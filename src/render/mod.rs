mod gpu_driven;
mod material;
mod shadows;
mod visibility;

use self::gpu_driven::GpuDrivenCulling;
use self::material::{PbrMaterial, PbrMaterialDesc};
use self::shadows::DirectionalShadowMap;
use self::visibility::{CullingMetrics, DrawRange, GameScene};
use crate::camera::GameCameraController;
use nord_app::{
    ApplicationBufferDesc, ApplicationBufferMemory, ApplicationBufferUsage, ApplicationClearColor,
    ApplicationColorAttachmentBinding, ApplicationDescriptorHandle, ApplicationDiagnosticError,
    ApplicationErrorContext, ApplicationMat4 as Mat4, ApplicationPipelineDescriptorBinding,
    ApplicationPlatform, ApplicationQuat as Quat, ApplicationRenderConfig, ApplicationRenderQueue,
    ApplicationRenderTargetDesc, ApplicationRenderer, ApplicationResourceUsage,
    ApplicationShaderBindingKind, ApplicationTextureFormat, ApplicationVec2 as Vec2,
    ApplicationVec3 as Vec3, ApplicationVec4 as Vec4, ApplicationVertexBufferBinding,
    ApplicationViewport, ApplicationWindowHandle,
};
use nord_camera::PerspectiveCamera;
use nord_mesh::{IndexedMesh, MeshVertex};

const CLEAR_COLOR: ApplicationClearColor = ApplicationClearColor::rgba(0.025, 0.035, 0.055, 1.0);
const VERTEX_STRIDE: u32 = 48;
const INSTANCE_STRIDE: u32 = 24;
const CONSTANT_BUFFER_BYTES: usize = 512;
const GAME_UV_TILE_COUNT: f32 = 2.0;

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
    instance_buffer: nord_app::ApplicationGpuResourceHandle,
    instance_buffer_size: u32,
    scene: GameScene,
    gpu_culling: GpuDrivenCulling,
    cpu_culling_baseline: CullingMetrics,
    gpu_validation_pending: bool,
    camera_buffer: nord_app::ApplicationGpuResourceHandle,
    camera_binding: ApplicationPipelineDescriptorBinding,
    camera: GameCameraController,
    model: Mat4,
    material: PbrMaterial,
    shadows: DirectionalShadowMap,
    shadow_visibility_mask: Vec<u8>,
    shadow_draw_ranges: Vec<DrawRange>,
    shadow_pass_reported: bool,
}

#[derive(Debug, Clone, Copy)]
struct GameVertex {
    position: Vec3,
    normal: Vec3,
    tangent: Vec4,
    tex_coord: Vec2,
}

struct GameMesh {
    validated_mesh: IndexedMesh,
    vertices: Vec<GameVertex>,
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
    #[allow(clippy::too_many_lines)]
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
        let material = PbrMaterial::load(
            &mut renderer,
            PbrMaterialDesc::new([0.74, 0.43, 0.19], 0.92, 0.82, 0.78, 1.0)?,
        )?;
        let shadows = DirectionalShadowMap::create(&mut renderer)?;
        let mesh = make_test_cube()?;
        let model = test_cube_transform()?;
        let mesh_gpu = Self::upload_test_mesh(&mut renderer, &mesh)?;
        let scene = GameScene::demo_grid(mesh.validated_mesh.bounds(), model);
        let (instance_buffer, instance_buffer_size) =
            Self::upload_instances(&mut renderer, &scene)?;
        let gpu_culling = GpuDrivenCulling::create(
            &mut renderer,
            instance_buffer,
            instance_buffer_size,
            u32::try_from(scene.instance_count())
                .map_err(ApplicationDiagnosticError::msg)
                .context("validating the scene instance count for GPU culling")?,
            mesh_gpu.index_count,
            scene.conservative_instance_radius().ok_or_else(|| {
                ApplicationDiagnosticError::msg(
                    "scene instance bounds cannot form a culling sphere",
                )
            })?,
        )?;
        let (camera_buffer, camera_binding) = Self::create_camera_buffer(&mut renderer)?;
        let surface = Self::create_surface(&mut renderer, width, height)?;
        let mut visible_mask = Vec::with_capacity(scene.instance_count());
        let mut draw_ranges = Vec::with_capacity(scene.instance_count());
        let initial_metrics = scene.update_visibility(
            camera
                .camera()
                .view_projection_matrix()
                .map_err(ApplicationDiagnosticError::msg)
                .context("building the initial game view-projection matrix for CPU culling")?,
            &mut visible_mask,
            &mut draw_ranges,
        );
        let (shadow_visibility_mask, shadow_draw_ranges, initial_shadow_metrics) =
            initial_shadow_visibility(&scene)?;
        let allocator_stats = renderer.gpu_allocator_stats();
        nord_app::debug!(
            target: "nord::render",
            reserved_gpu_bytes = allocator_stats.reserved_bytes,
            "Renderer resource bookkeeping services are ready"
        );
        nord_app::info!(
            name: "pbr_mesh_render_path_initialized",
            target: "nord::render",
            backend = ?renderer.backend(),
            vertices = mesh_gpu.vertex_count,
            indices = mesh_gpu.index_count,
            "Game camera, indexed mesh, metallic-roughness material, normal map, and depth-tested pass are ready"
        );
        nord_app::info!(
            name: "directional_shadow_path_initialized",
            target: "nord::render",
            resolution = DirectionalShadowMap::resolution(),
            caster_instances = initial_shadow_metrics.visible_instances,
            shadow_draw_ranges = initial_shadow_metrics.instanced_draw_calls,
            "Directional light shadow map is configured as a depth-tested render-graph pass"
        );
        nord_app::info!(
            name: "cpu_visibility_instancing_metrics",
            target: "nord::render",
            total_instances = initial_metrics.total_instances,
            visible_instances = initial_metrics.visible_instances,
            culled_instances = initial_metrics.culled_instances,
            naive_draw_calls = initial_metrics.naive_draw_calls,
            instanced_draw_calls = initial_metrics.instanced_draw_calls,
            culling_ns = initial_metrics.culling_ns,
            grouping_ns = initial_metrics.grouping_ns,
            "CPU frustum culling and mesh/material batching measured for the initial camera"
        );
        nord_app::info!(
            name: "gpu_driven_path_initialized",
            target: "nord::render",
            input_instances = initial_metrics.total_instances,
            cpu_visible_baseline = initial_metrics.visible_instances,
            cpu_culling_ns = initial_metrics.culling_ns,
            indirect_draw_commands = 1,
            "GPU frustum compaction and indexed-indirect rendering are configured for the camera pass"
        );
        Ok(Self {
            renderer: Some(renderer),
            surface,
            vertex_buffer: mesh_gpu.vertex_buffer,
            vertex_buffer_size: mesh_gpu.vertex_buffer_size,
            index_buffer: mesh_gpu.index_buffer,
            index_buffer_size: mesh_gpu.index_buffer_size,
            index_count: mesh_gpu.index_count,
            instance_buffer,
            instance_buffer_size,
            scene,
            gpu_culling,
            cpu_culling_baseline: initial_metrics,
            gpu_validation_pending: std::env::var_os("NORD_GPU_DRIVEN_VALIDATE").is_some(),
            camera_buffer,
            camera_binding,
            camera,
            model,
            material,
            shadows,
            shadow_visibility_mask,
            shadow_draw_ranges,
            shadow_pass_reported: false,
        })
    }

    fn upload_test_mesh(
        renderer: &mut ApplicationRenderer,
        mesh: &GameMesh,
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
            vertex_count: mesh.vertices.len(),
        })
    }

    fn upload_instances(
        renderer: &mut ApplicationRenderer,
        scene: &GameScene,
    ) -> nord_app::ApplicationResult<(nord_app::ApplicationGpuResourceHandle, u32)> {
        let bytes = scene.instance_bytes();
        let buffer_size = u32::try_from(bytes.len())
            .map_err(ApplicationDiagnosticError::msg)
            .context("checking the game's instance-buffer size")?;
        if buffer_size == 0 || buffer_size % INSTANCE_STRIDE != 0 {
            return Err(ApplicationDiagnosticError::msg(
                "game scene produced an empty or misaligned instance-data buffer",
            ));
        }
        let buffer = renderer
            .create_buffer(ApplicationBufferDesc::new(
                u64::from(buffer_size),
                ApplicationBufferUsage::COPY_DESTINATION
                    .union(ApplicationBufferUsage::VERTEX)
                    .union(ApplicationBufferUsage::STORAGE),
            )?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating the game's static instance-data buffer")?;
        renderer
            .upload_buffer(buffer, 0, bytes)
            .map_err(ApplicationDiagnosticError::msg)
            .context("uploading the game's static instance data")?;
        Ok((buffer, buffer_size))
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
        let camera_buffer = self.camera_buffer;
        let view_projection = self
            .camera
            .camera()
            .view_projection_matrix()
            .map_err(ApplicationDiagnosticError::msg)
            .context("building the game's view-projection matrix")?;
        let frustum_planes = visibility::extract_frustum_planes(view_projection)
            .ok_or_else(|| ApplicationDiagnosticError::msg("camera frustum planes are invalid"))?;
        let light_view_projection =
            shadows::directional_light_view_projection().ok_or_else(|| {
                ApplicationDiagnosticError::msg("directional light projection is invalid")
            })?;
        let _shadow_visibility_metrics = self.scene.update_visibility(
            light_view_projection,
            &mut self.shadow_visibility_mask,
            &mut self.shadow_draw_ranges,
        );
        let shadow_draw_ranges = self.shadow_draw_ranges.clone();
        let normal_model =
            self.model.inverse().map(Mat4::transpose).ok_or_else(|| {
                ApplicationDiagnosticError::msg("game object transform is singular")
            })?;
        let constants = encode_camera_constants(
            self.model,
            view_projection,
            normal_model,
            self.camera.camera().position(),
            light_view_projection,
        );
        let renderer = self
            .renderer
            .as_mut()
            .ok_or_else(|| ApplicationDiagnosticError::msg("renderer has already shut down"))?;
        renderer
            .write_buffer(camera_buffer, 0, &constants)
            .map_err(ApplicationDiagnosticError::msg)
            .context("updating the game's per-frame camera and model matrices")?;
        self.gpu_culling.update_constants(renderer, frustum_planes)?;
        self.submit_pbr_mesh_pass(shadow_draw_ranges)
    }

    #[allow(clippy::too_many_lines)] // One render-graph pass owns its declarations, encoding and submission.
    fn submit_pbr_mesh_pass(
        &mut self,
        shadow_draw_ranges: Vec<DrawRange>,
    ) -> nord_app::ApplicationResult<()> {
        let renderer = self
            .renderer
            .as_mut()
            .ok_or_else(|| ApplicationDiagnosticError::msg("renderer has already shut down"))?;
        let surface = self.surface;
        let vertex_buffer = self.vertex_buffer;
        let vertex_buffer_size = self.vertex_buffer_size;
        let index_buffer = self.index_buffer;
        let index_buffer_size = self.index_buffer_size;
        let instance_buffer = self.instance_buffer;
        let instance_buffer_size = self.instance_buffer_size;
        let visible_instances = self.gpu_culling.visible_instances();
        let visible_instances_size = self.gpu_culling.visible_instances_size();
        let indirect_arguments = self.gpu_culling.arguments();
        let maximum_instances = self.gpu_culling.maximum_instances();
        let camera_buffer = self.camera_buffer;
        let camera_binding = self.camera_binding;
        let material_constants = self.material.constants_resource();
        let material_textures = self.material.texture_resources();
        let material_bindings = *self.material.bindings();
        let shadow_map = self.shadows.resources();
        let shadow_pipeline = self.shadows.pipeline().clone();
        let shadow_vertex_shader = self.shadows.vertex_shader().clone();
        let shadow_pixel_shader = self.shadows.pixel_shader().clone();
        let shadow_binding = self.shadows.sampled_binding();
        let index_count = self.index_count;
        let pipeline = self.material.pipeline().clone();
        let vertex_shader = self.material.vertex_shader().clone();
        let pixel_shader = self.material.pixel_shader().clone();
        let viewport_width = exact_surface_extent(surface.width)?;
        let viewport_height = exact_surface_extent(surface.height)?;
        let shadow_extent = exact_surface_extent(DirectionalShadowMap::resolution())?;
        let mut queue = ApplicationRenderQueue::new();
        self.gpu_culling.enqueue_compute_passes(&mut queue)?;
        queue
            .enqueue(
                nord_app::ApplicationRenderPassDesc::new("Nord directional shadow depth")
                    .use_resource(shadow_map.color, ApplicationResourceUsage::ColorAttachmentWrite)
                    .use_resource(shadow_map.depth, ApplicationResourceUsage::DepthAttachmentWrite)
                    .use_resource(vertex_buffer, ApplicationResourceUsage::VertexBufferRead)
                    .use_resource(index_buffer, ApplicationResourceUsage::IndexBufferRead)
                    .use_resource(instance_buffer, ApplicationResourceUsage::VertexBufferRead)
                    .use_resource(camera_buffer, ApplicationResourceUsage::ConstantBufferRead),
                move |encoder| {
                    encoder.begin_graphics_pass_with_attachments(
                        &[ApplicationColorAttachmentBinding {
                            resource: shadow_map.color,
                            view: shadow_map.color_view,
                            clear_color: ApplicationClearColor::rgba(1.0, 1.0, 1.0, 1.0),
                        }],
                        Some((shadow_map.depth, shadow_map.depth_view)),
                        1.0,
                        Some(ApplicationViewport::new(0.0, 0.0, shadow_extent, shadow_extent)),
                        None,
                    )?;
                    for range in shadow_draw_ranges {
                        encoder.draw(
                            &shadow_pipeline,
                            &shadow_vertex_shader,
                            &shadow_pixel_shader,
                            &[camera_binding],
                            &[
                                ApplicationVertexBufferBinding {
                                    slot: 0,
                                    buffer: vertex_buffer,
                                    offset: 0,
                                    size: vertex_buffer_size,
                                    stride: VERTEX_STRIDE,
                                },
                                ApplicationVertexBufferBinding {
                                    slot: 1,
                                    buffer: instance_buffer,
                                    offset: 0,
                                    size: instance_buffer_size,
                                    stride: INSTANCE_STRIDE,
                                },
                            ],
                            Some(nord_app::ApplicationIndexBufferBinding {
                                buffer: index_buffer,
                                offset: 0,
                                size: index_buffer_size,
                                format: nord_app::ApplicationIndexFormat::Uint16,
                            }),
                            nord_app::ApplicationDrawCall::Indexed {
                                index_count,
                                instance_count: range.instance_count,
                                first_index: 0,
                                base_vertex: 0,
                                first_instance: range.first_instance,
                            },
                        )?;
                    }
                    encoder.end_graphics_pass()?;
                    Ok(())
                },
            )
            .map_err(|error| ApplicationDiagnosticError::msg(format!("{error:?}")))
            .context("encoding the game's directional-light shadow depth pass")?;
        queue
            .enqueue(
                nord_app::ApplicationRenderPassDesc::new("Nord metallic-roughness PBR mesh")
                    .use_resource(surface.color, ApplicationResourceUsage::ColorAttachmentWrite)
                    .use_resource(surface.depth, ApplicationResourceUsage::DepthAttachmentWrite)
                    .use_resource(vertex_buffer, ApplicationResourceUsage::VertexBufferRead)
                    .use_resource(index_buffer, ApplicationResourceUsage::IndexBufferRead)
                    .use_resource(visible_instances, ApplicationResourceUsage::VertexBufferRead)
                    .use_resource(
                        indirect_arguments,
                        ApplicationResourceUsage::IndirectArgumentRead,
                    )
                    .use_resource(camera_buffer, ApplicationResourceUsage::ConstantBufferRead)
                    .use_resource(material_constants, ApplicationResourceUsage::ConstantBufferRead)
                    .use_resource(material_textures[0], ApplicationResourceUsage::SampledRead)
                    .use_resource(material_textures[1], ApplicationResourceUsage::SampledRead)
                    .use_resource(material_textures[2], ApplicationResourceUsage::SampledRead)
                    .use_resource(shadow_map.color, ApplicationResourceUsage::SampledRead),
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
                    encoder.draw_indexed_indirect(
                        &pipeline,
                        &vertex_shader,
                        &pixel_shader,
                        &[
                            camera_binding,
                            material_bindings[0],
                            material_bindings[1],
                            material_bindings[2],
                            material_bindings[3],
                            material_bindings[4],
                            shadow_binding,
                        ],
                        &[
                            ApplicationVertexBufferBinding {
                                slot: 0,
                                buffer: vertex_buffer,
                                offset: 0,
                                size: vertex_buffer_size,
                                stride: VERTEX_STRIDE,
                            },
                            ApplicationVertexBufferBinding {
                                slot: 1,
                                buffer: visible_instances,
                                offset: 0,
                                size: visible_instances_size,
                                stride: INSTANCE_STRIDE,
                            },
                        ],
                        nord_app::ApplicationIndexBufferBinding {
                            buffer: index_buffer,
                            offset: 0,
                            size: index_buffer_size,
                            format: nord_app::ApplicationIndexFormat::Uint16,
                        },
                        indirect_arguments,
                        0,
                        1,
                        maximum_instances,
                        index_count,
                    )?;
                    encoder.end_graphics_pass()?;
                    Ok(())
                },
            )
            .map_err(|error| ApplicationDiagnosticError::msg(format!("{error:?}")))
            .context("encoding the game's metallic-roughness PBR mesh pass")?;
        renderer
            .submit_render_queue(&mut queue)
            .map_err(|error| ApplicationDiagnosticError::msg(format!("{error:?}")))
            .context("submitting the game's PBR mesh render graph")?;
        if self.gpu_validation_pending {
            let visible_instances = self.gpu_culling.validate_gpu_output(renderer)?;
            nord_app::info!(
                name: "gpu_driven_culling_validated",
                target: "nord::render",
                input_instances = self.cpu_culling_baseline.total_instances,
                visible_instances,
                culled_instances = self
                    .cpu_culling_baseline
                    .total_instances
                    .saturating_sub(visible_instances),
                cpu_visible_baseline = self.cpu_culling_baseline.visible_instances,
                cpu_culling_ns = self.cpu_culling_baseline.culling_ns,
                indirect_draw_commands = 1,
                "GPU-generated draw arguments and compacted instance count passed readback validation"
            );
            self.gpu_validation_pending = false;
        }
        if !self.shadow_pass_reported {
            nord_app::info!(
                name: "directional_shadow_pass_submitted",
                target: "nord::render",
                resolution = DirectionalShadowMap::resolution(),
                "Directional shadow depth and forward PBR passes were submitted in graph order"
            );
            self.shadow_pass_reported = true;
        }
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
    ) -> nord_app::ApplicationResult<crate::camera::GameCursorAction> {
        self.camera.handle_input(event)
    }

    pub(super) fn handle_window_focus(&mut self, focused: bool) -> crate::camera::GameCursorAction {
        self.camera.handle_window_focus(focused)
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
        self.material.shutdown(&mut renderer)?;
        self.shadows.shutdown(&mut renderer)?;
        self.gpu_culling.destroy(&mut renderer)?;
        for resource in [
            self.surface.color,
            self.surface.depth,
            self.vertex_buffer,
            self.index_buffer,
            self.instance_buffer,
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

fn initial_shadow_visibility(
    scene: &GameScene,
) -> nord_app::ApplicationResult<(Vec<u8>, Vec<DrawRange>, visibility::CullingMetrics)> {
    let view_projection = shadows::directional_light_view_projection().ok_or_else(|| {
        ApplicationDiagnosticError::msg("directional light projection is invalid")
    })?;
    let mut visible_mask = Vec::with_capacity(scene.instance_count());
    let mut draw_ranges = Vec::with_capacity(scene.instance_count());
    let metrics = scene.update_visibility(view_projection, &mut visible_mask, &mut draw_ranges);
    Ok((visible_mask, draw_ranges, metrics))
}

fn test_cube_transform() -> nord_app::ApplicationResult<Mat4> {
    let rotation = Quat::from_axis_angle(Vec3::Y, 0.65)
        .zip(Quat::from_axis_angle(Vec3::X, -0.35))
        .map(|(yaw, pitch)| yaw * pitch)
        .ok_or_else(|| ApplicationDiagnosticError::msg("test cube rotation is invalid"))?;
    Mat4::from_translation_rotation_scale(Vec3::ZERO, rotation, Vec3::splat(0.9))
        .ok_or_else(|| ApplicationDiagnosticError::msg("test cube transform is invalid"))
}

fn make_test_cube() -> nord_app::ApplicationResult<GameMesh> {
    let mut vertices = Vec::with_capacity(24);
    let mut validated_vertices = Vec::with_capacity(24);
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
        let tangent = (corners[1] - corners[0])
            .try_normalize()
            .ok_or_else(|| ApplicationDiagnosticError::msg("cube face tangent is degenerate"))?;
        let bitangent = (corners[3] - corners[0])
            .try_normalize()
            .ok_or_else(|| ApplicationDiagnosticError::msg("cube face bitangent is degenerate"))?;
        let handedness = if normal.cross(tangent).dot(bitangent) < 0.0 { -1.0 } else { 1.0 };
        let tex_coords = [
            Vec2::new(0.0, 0.0),
            Vec2::new(GAME_UV_TILE_COUNT, 0.0),
            Vec2::new(GAME_UV_TILE_COUNT, GAME_UV_TILE_COUNT),
            Vec2::new(0.0, GAME_UV_TILE_COUNT),
        ];
        for (corner_index, position) in corners.into_iter().enumerate() {
            let validated = MeshVertex::new(position, normal)
                .map_err(ApplicationDiagnosticError::msg)
                .context("validating the game's cube vertex")?;
            validated_vertices.push(validated);
            vertices.push(GameVertex {
                position,
                normal,
                tangent: Vec4::new(tangent.x, tangent.y, tangent.z, handedness),
                tex_coord: tex_coords[corner_index],
            });
        }
        indices.extend_from_slice(&[first, first + 1, first + 2, first, first + 2, first + 3]);
    }
    let validated_mesh = IndexedMesh::new(validated_vertices, indices)
        .map_err(ApplicationDiagnosticError::msg)
        .context("validating the game's indexed cube mesh")?;
    Ok(GameMesh { validated_mesh, vertices })
}

fn encode_mesh(mesh: &GameMesh) -> nord_app::ApplicationResult<(Vec<u8>, Vec<u8>, u32)> {
    let mut vertices = Vec::with_capacity(mesh.vertices.len() * VERTEX_STRIDE as usize);
    for vertex in &mesh.vertices {
        for value in vertex
            .position
            .to_array()
            .into_iter()
            .chain(vertex.normal.to_array())
            .chain(vertex.tangent.to_array())
            .chain(vertex.tex_coord.to_array())
        {
            vertices.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut indices = Vec::with_capacity(mesh.validated_mesh.indices().len() * 2);
    for &index in mesh.validated_mesh.indices() {
        let index = u16::try_from(index)
            .map_err(|_| ApplicationDiagnosticError::msg("test mesh exceeds 16-bit indices"))?;
        indices.extend_from_slice(&index.to_le_bytes());
    }
    let index_count = u32::try_from(mesh.validated_mesh.indices().len())
        .map_err(|_| ApplicationDiagnosticError::msg("test mesh has too many indices"))?;
    Ok((vertices, indices, index_count))
}

fn encode_camera_constants(
    model: Mat4,
    view_projection: Mat4,
    normal_model: Mat4,
    camera_position: Vec3,
    light_view_projection: Mat4,
) -> [u8; CONSTANT_BUFFER_BYTES] {
    let mut bytes = [0_u8; CONSTANT_BUFFER_BYTES];
    let mut offset = 0;
    for matrix in [model, view_projection, normal_model] {
        for column in matrix.to_cols() {
            for value in column.to_array() {
                bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                offset += 4;
            }
        }
    }
    for value in [camera_position.x, camera_position.y, camera_position.z, 1.0] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        offset += 4;
    }
    for column in light_view_projection.to_cols() {
        for value in column.to_array() {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            offset += 4;
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
