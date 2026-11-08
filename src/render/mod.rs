mod crosshair;
mod fps_hud;
mod gpu_driven;
mod material;
mod occlusion;
mod shadows;
mod sky;
mod visibility;
mod voxel;

use self::crosshair::Crosshair;
use self::fps_hud::FpsHud;
use self::gpu_driven::GpuDrivenCulling;
use self::material::{BlockTextureMaterials, PbrMaterial, PbrMaterialDesc};
use self::occlusion::HiZOcclusion;
use self::shadows::DirectionalShadowMap;
use self::sky::Sky;
use self::visibility::{CullingMetrics, DrawRange, GameScene};
use self::voxel::{VoxelDraw, VoxelRenderer};
use crate::camera::GameCameraController;
use nord_app::{
    ApplicationBufferDesc, ApplicationBufferMemory, ApplicationBufferUsage, ApplicationClearColor,
    ApplicationColorAttachmentBinding, ApplicationDescriptorHandle, ApplicationDiagnosticError,
    ApplicationDrawCall, ApplicationErrorContext, ApplicationFrustum3 as Frustum3,
    ApplicationGpuFrameTiming, ApplicationJobSystem, ApplicationMat4 as Mat4,
    ApplicationPipelineDescriptorBinding, ApplicationPlatform, ApplicationRenderConfig,
    ApplicationRenderQueue, ApplicationRenderQueueTiming, ApplicationRenderTargetDesc,
    ApplicationRenderer, ApplicationResourceUsage, ApplicationShaderBindingKind,
    ApplicationShutdownMode, ApplicationTaskScheduler, ApplicationTextureFormat,
    ApplicationVec2 as Vec2, ApplicationVec3 as Vec3, ApplicationVec4 as Vec4,
    ApplicationVertexBufferBinding, ApplicationViewport, ApplicationWindowHandle,
};
use nord_camera::PerspectiveCamera;
use nord_mesh::{IndexedMesh, MeshVertex};
use nord_world::BlockPosition;

const CLEAR_COLOR: ApplicationClearColor = ApplicationClearColor::rgba(0.025, 0.035, 0.055, 1.0);
const VERTEX_STRIDE: u32 = 48;
const INSTANCE_STRIDE: u32 = 24;
const CONSTANT_BUFFER_BYTES: usize = 512;
const GAME_UV_TILE_COUNT: f32 = 2.0;
const RENDER_BENCHMARK_WARMUP_FRAMES: usize = 16;
const RENDER_BENCHMARK_MEASURED_FRAMES: usize = 80;

#[derive(Clone, Copy)]
struct BenchmarkFrame {
    frame_ns: u128,
    graph_ns: u128,
    present_ns: u128,
    stream_ns: u128,
    mesh_sync_ns: u128,
    queue: ApplicationRenderQueueTiming,
}

/// Surface-sized game output and its depth buffer.
#[derive(Clone, Copy)]
struct GameSurface {
    scene_color: nord_app::ApplicationGpuResourceHandle,
    scene_color_view: ApplicationDescriptorHandle,
    scene_color_srv: ApplicationDescriptorHandle,
    color: nord_app::ApplicationGpuResourceHandle,
    color_view: ApplicationDescriptorHandle,
    depth: nord_app::ApplicationGpuResourceHandle,
    depth_view: ApplicationDescriptorHandle,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SurfaceResizeAction {
    Suspend,
    Resume,
    Unchanged,
    Recreate,
}

fn surface_resize_action(
    current_width: u32,
    current_height: u32,
    width: u32,
    height: u32,
    surface_suspended: bool,
) -> SurfaceResizeAction {
    if width == 0 || height == 0 {
        SurfaceResizeAction::Suspend
    } else if width == current_width && height == current_height {
        if surface_suspended { SurfaceResizeAction::Resume } else { SurfaceResizeAction::Unchanged }
    } else {
        SurfaceResizeAction::Recreate
    }
}

/// Game-owned policy that connects Nord's generic renderer to the app loop.
pub(crate) struct GameRenderServices {
    pub(crate) jobs: ApplicationJobSystem,
    pub(crate) scheduler: ApplicationTaskScheduler,
    pub(crate) chunks_above_origin: i32,
    pub(crate) chunks_below_origin: i32,
}

pub(super) struct GameRenderer {
    jobs: ApplicationJobSystem,
    scheduler: ApplicationTaskScheduler,
    voxel_world: crate::world::VoxelWorld,
    chunk_streamer: crate::world::ChunkStreamer,
    _cpu_memory_reservation: nord_app::ApplicationMemoryReservation,
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
    occlusion: Option<HiZOcclusion>,
    benchmark_pending: bool,
    benchmark_samples: Vec<BenchmarkFrame>,
    cpu_culling_baseline: CullingMetrics,
    gpu_validation_pending: bool,
    camera_buffer: nord_app::ApplicationGpuResourceHandle,
    camera_binding: ApplicationPipelineDescriptorBinding,
    camera: GameCameraController,
    model: Mat4,
    crosshair: Crosshair,
    fps_hud: FpsHud,
    sky: Sky,
    material: PbrMaterial,
    block_textures: BlockTextureMaterials,
    voxel_renderer: VoxelRenderer,
    demo_scene_enabled: bool,
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
        services: GameRenderServices,
        memory: &nord_app::ApplicationMemoryManager,
    ) -> nord_app::ApplicationResult<Self> {
        let GameRenderServices { jobs, scheduler, chunks_above_origin, chunks_below_origin } =
            services;
        let demo_scene_enabled =
            std::env::var("NORD_RENDER_DEMO_SCENE").is_ok_and(|value| value == "1");
        let vertical_sync = std::env::var("NORD_VSYNC")
            .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
        let aspect_ratio = surface_aspect(width, height)?;
        let camera = GameCameraController::new(create_game_camera(aspect_ratio)?, window);
        let voxel_world = crate::world::VoxelWorld::with_vertical_bounds(
            crate::world::BlockRegistry::built_in()
                .map_err(ApplicationDiagnosticError::msg)
                .context("loading the built-in game block registry")?,
            chunks_above_origin,
            chunks_below_origin,
        );
        let chunk_streamer = crate::world::ChunkStreamer::new(
            crate::world::ChunkStreamConfig::new(chunks_above_origin, chunks_below_origin),
            &voxel_world,
        );
        let mut renderer = ApplicationRenderer::initialize(
            platform,
            window,
            ApplicationRenderConfig::new(width, height).with_vertical_sync(vertical_sync),
        )
        .map_err(ApplicationDiagnosticError::msg)
        .context("initializing Nord's graphics infrastructure")?;
        let material = PbrMaterial::load(
            &mut renderer,
            PbrMaterialDesc::new([1.0, 1.0, 1.0], 0.0, 1.0, 0.0, 1.0)?,
            demo_scene_enabled,
        )?;
        let block_textures = BlockTextureMaterials::create(&mut renderer)?;
        let voxel_renderer = VoxelRenderer::create(&mut renderer)?;
        let crosshair = Crosshair::create(&mut renderer, width, height)?;
        let fps_hud = FpsHud::create(&mut renderer)?;
        let sky = Sky::create(&mut renderer)?;
        let shadows = DirectionalShadowMap::create(
            &mut renderer,
            if demo_scene_enabled { DirectionalShadowMap::resolution() } else { 1 },
        )?;
        let mesh = make_test_cube()?;
        let model = Mat4::IDENTITY;
        let mesh_gpu = Self::upload_test_mesh(&mut renderer, &mesh)?;
        let occlusion_probe_enabled = demo_scene_enabled
            && std::env::var("NORD_OCCLUSION_STRESS_SCENE").is_ok_and(|value| value == "1");
        let instance_capacity = GameScene::demo_instance_count()
            .checked_add(if occlusion_probe_enabled { 8 } else { 0 })
            .ok_or_else(|| {
                ApplicationDiagnosticError::msg("game scene instance count overflowed")
            })?;
        let estimated_cpu_memory_bytes = [
            GameScene::estimated_retained_cpu_bytes(instance_capacity).ok_or_else(|| {
                ApplicationDiagnosticError::msg("game scene CPU memory estimate overflowed")
            })?,
            vector_capacity_bytes::<u8>(instance_capacity)?,
            vector_capacity_bytes::<DrawRange>(instance_capacity)?,
            vector_capacity_bytes::<u128>(96)?,
        ]
        .into_iter()
        .try_fold(0_u64, u64::checked_add)
        .ok_or_else(|| {
            ApplicationDiagnosticError::msg("game renderer CPU memory estimate overflowed")
        })?;
        let mut cpu_memory_reservation = memory
            .reserve(estimated_cpu_memory_bytes)
            .map_err(ApplicationDiagnosticError::msg)
            .context("reserving memory before growing retained game CPU buffers")?;
        let mut scene =
            GameScene::demo_grid(mesh.validated_mesh.bounds(), model, instance_capacity);
        let occlusion_probe_added =
            occlusion_probe_enabled && scene.add_occlusion_probe(camera.camera().forward());
        if occlusion_probe_enabled && !occlusion_probe_added {
            return Err(ApplicationDiagnosticError::msg(
                "could not create the requested conservative occlusion stress scene",
            ));
        }
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
            scene.conservative_instance_half_extents().ok_or_else(|| {
                ApplicationDiagnosticError::msg(
                    "scene instance bounds cannot form conservative culling extents",
                )
            })?,
        )?;
        let (camera_buffer, camera_binding) = Self::create_camera_buffer(&mut renderer)?;
        let surface = Self::create_surface(&mut renderer, width, height)?;
        let occlusion_enabled = demo_scene_enabled && occlusion_culling_enabled();
        let occlusion = if occlusion_enabled {
            Some(HiZOcclusion::create(&mut renderer, width, height)?)
        } else {
            None
        };
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
        let benchmark_samples = Vec::with_capacity(96);
        let tracked_cpu_memory_bytes = [
            scene.retained_cpu_bytes().ok_or_else(|| {
                ApplicationDiagnosticError::msg("game scene CPU memory accounting overflowed")
            })?,
            vector_capacity_bytes::<u8>(shadow_visibility_mask.capacity())?,
            vector_capacity_bytes::<DrawRange>(shadow_draw_ranges.capacity())?,
            vector_capacity_bytes::<BenchmarkFrame>(benchmark_samples.capacity())?,
        ]
        .into_iter()
        .try_fold(0_u64, u64::checked_add)
        .ok_or_else(|| {
            ApplicationDiagnosticError::msg("game renderer CPU memory accounting overflowed")
        })?;
        cpu_memory_reservation
            .resize(tracked_cpu_memory_bytes)
            .map_err(ApplicationDiagnosticError::msg)
            .context("reconciling reservations with retained game CPU buffer capacities")?;
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
        nord_app::info!(
            name: "occlusion_path_selected",
            target: "nord::render",
            path = if occlusion_enabled { "same-frame-hiz" } else { "frustum-only" },
            occlusion_probe_instances = if occlusion_probe_added { 8 } else { 0 },
            "Occlusion path selected for this run"
        );
        Ok(Self {
            jobs,
            scheduler,
            voxel_world,
            chunk_streamer,
            _cpu_memory_reservation: cpu_memory_reservation,
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
            occlusion,
            benchmark_pending: std::env::var_os("NORD_RENDER_BENCHMARK").is_some(),
            benchmark_samples,
            cpu_culling_baseline: initial_metrics,
            gpu_validation_pending: demo_scene_enabled
                && std::env::var_os("NORD_GPU_DRIVEN_VALIDATE").is_some(),
            camera_buffer,
            camera_binding,
            camera,
            model,
            crosshair,
            fps_hud,
            sky,
            material,
            block_textures,
            voxel_renderer,
            demo_scene_enabled,
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
            .context("creating the game's scene color and depth targets")?;
        let scene_color = target.color.expect("scene color target was requested");
        let depth = target.depth.expect("depth target was requested");
        let scene_color_view = renderer
            .create_render_target_view(scene_color)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's scene color attachment view")?;
        let scene_color_srv = renderer
            .create_texture_srv(scene_color)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the sampled scene color view for adaptive crosshair shading")?;
        let depth_view = renderer
            .create_depth_stencil_view(depth)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's depth attachment view")?;
        let composite = renderer
            .create_render_target(ApplicationRenderTargetDesc::new(
                width,
                height,
                Some(ApplicationTextureFormat::Rgba8Unorm),
                None,
            )?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the composited game output target")?;
        let color = composite.color.expect("composite color target was requested");
        let color_view = renderer
            .create_render_target_view(color)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the composited output attachment view")?;
        Ok(GameSurface {
            scene_color,
            scene_color_view,
            scene_color_srv,
            color,
            color_view,
            depth,
            depth_view,
            width,
            height,
        })
    }

    pub(super) fn render_frame(
        &mut self,
        fps: Option<nord_app::ApplicationFpsSnapshot>,
    ) -> nord_app::ApplicationResult<()> {
        let frame_start = self.benchmark_pending.then(std::time::Instant::now);
        let camera_buffer = self.camera_buffer;
        let view_projection = self
            .camera
            .camera()
            .view_projection_matrix()
            .map_err(ApplicationDiagnosticError::msg)
            .context("building the game's view-projection matrix")?;
        let frustum_planes = visibility::extract_frustum_planes(view_projection)
            .ok_or_else(|| ApplicationDiagnosticError::msg("camera frustum planes are invalid"))?;
        let camera_position = self.camera.camera().position();
        let focus = BlockPosition::new(
            floor_block_coordinate(camera_position.x)?,
            floor_block_coordinate(camera_position.y)?,
            floor_block_coordinate(camera_position.z)?,
        );
        let previous_stream_focus = self.chunk_streamer.snapshot().focus_chunk;
        let stream_start = frame_start.map(|_| std::time::Instant::now());
        let stream_snapshot = self.chunk_streamer.update(
            &mut self.voxel_world,
            &self.scheduler,
            focus,
            Frustum3::from_planes(frustum_planes),
        );
        let stream_ns = stream_start.map_or(0, |start| start.elapsed().as_nanos());
        let frustum = Frustum3::from_planes(frustum_planes);
        if previous_stream_focus != stream_snapshot.focus_chunk {
            nord_app::info!(
                name: "voxel_chunk_stream_focus_changed",
                target: "nord::world::streaming",
                focus_chunk = ?stream_snapshot.focus_chunk,
                desired_chunks = stream_snapshot.desired_chunks,
                visible_desired_chunks = stream_snapshot.visible_desired_chunks,
                load_radius_chunks = 8,
                vertical_load_radius_chunks = 2,
                "Updated the desired flat-world chunk region"
            );
        }
        let mesh_sync_start = frame_start.map(|_| std::time::Instant::now());
        if let Some(renderer) = self.renderer.as_mut() {
            self.voxel_renderer.sync(
                renderer,
                &mut self.voxel_world,
                &self.block_textures,
                focus.chunk(),
            )?;
        }
        let mesh_sync_ns = mesh_sync_start.map_or(0, |start| start.elapsed().as_nanos());
        let voxel_draws = self.voxel_renderer.visible_draws(frustum);
        let visible_voxel_draws = voxel_draws.len();
        let benchmark_start = frame_start.filter(|_| {
            self.benchmark_pending
                && stream_snapshot.loaded_chunks == stream_snapshot.desired_chunks
                && stream_snapshot.generation_jobs == 0
                && !self.voxel_renderer.has_pending_meshes()
        });
        let light_view_projection =
            shadows::directional_light_view_projection().ok_or_else(|| {
                ApplicationDiagnosticError::msg("directional light projection is invalid")
            })?;
        let shadow_draw_ranges = if self.demo_scene_enabled {
            self.scene.update_visibility(
                light_view_projection,
                &mut self.shadow_visibility_mask,
                &mut self.shadow_draw_ranges,
            );
            self.shadow_draw_ranges.clone()
        } else {
            Vec::new()
        };
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
        let fps_vertex_count = self.fps_hud.update(
            renderer,
            self.surface.width,
            self.surface.height,
            fps.map(|snapshot| snapshot.frames_per_second),
        )?;
        if self.demo_scene_enabled {
            self.gpu_culling.update_constants(
                renderer,
                frustum_planes,
                view_projection,
                self.occlusion.as_ref().map_or(1, HiZOcclusion::mip_count),
            )?;
        }
        self.submit_pbr_mesh_pass(
            shadow_draw_ranges,
            voxel_draws,
            visible_voxel_draws,
            benchmark_start,
            fps_vertex_count,
            stream_ns,
            mesh_sync_ns,
        )
    }

    #[allow(clippy::too_many_lines)] // One render-graph pass owns its declarations, encoding and submission.
    fn submit_pbr_mesh_pass(
        &mut self,
        shadow_draw_ranges: Vec<DrawRange>,
        voxel_draws: std::sync::Arc<[VoxelDraw]>,
        visible_voxel_draws: usize,
        benchmark_start: Option<std::time::Instant>,
        fps_vertex_count: u32,
        stream_ns: u128,
        mesh_sync_ns: u128,
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
        let voxel_instance_buffer = self.voxel_renderer.instance_buffer();
        let voxel_instance_size = self.voxel_renderer.instance_size();
        let draw_demo_scene = self.demo_scene_enabled;
        let shadow_map = self.shadows.resources();
        let shadow_pipeline = self.shadows.pipeline().clone();
        let shadow_vertex_shader = self.shadows.vertex_shader().clone();
        let shadow_pixel_shader = self.shadows.pixel_shader().clone();
        let shadow_binding = self.shadows.sampled_binding();
        let crosshair_buffer = self.crosshair.vertex_buffer();
        let crosshair_pipeline = self.crosshair.pipeline().clone();
        let crosshair_vertex_shader = self.crosshair.vertex_shader().clone();
        let crosshair_pixel_shader = self.crosshair.pixel_shader().clone();
        let fps_pipeline = self.fps_hud.pipeline().clone();
        let fps_vertex_shader = self.fps_hud.vertex_shader().clone();
        let fps_pixel_shader = self.fps_hud.pixel_shader().clone();
        let fps_vertex_buffer = self.fps_hud.vertex_buffer();
        let crosshair_blit_pipeline = self.crosshair.blit_pipeline().clone();
        let crosshair_blit_vertex_shader = self.crosshair.blit_vertex_shader().clone();
        let crosshair_blit_pixel_shader = self.crosshair.blit_pixel_shader().clone();
        let sky_buffer = self.sky.vertex_buffer();
        let sky_pipeline = self.sky.pipeline().clone();
        let sky_vertex_shader = self.sky.vertex_shader().clone();
        let sky_pixel_shader = self.sky.pixel_shader().clone();
        let index_count = self.index_count;
        let pipeline = self.material.pipeline().clone();
        let vertex_shader = self.material.vertex_shader().clone();
        let pixel_shader = self.material.pixel_shader().clone();
        let viewport_width = exact_surface_extent(surface.width)?;
        let viewport_height = exact_surface_extent(surface.height)?;
        let shadow_extent = exact_surface_extent(self.shadows.active_resolution())?;
        let mut queue = ApplicationRenderQueue::new().with_inline_encoding();
        if draw_demo_scene {
            if let Some(occlusion) = &self.occlusion {
                self.gpu_culling.enqueue_compute_passes(&mut queue)?;
                occlusion.enqueue_depth_prepass(
                    &mut queue,
                    camera_buffer,
                    camera_binding,
                    vertex_buffer,
                    vertex_buffer_size,
                    index_buffer,
                    index_buffer_size,
                    index_count,
                    visible_instances,
                    visible_instances_size,
                    indirect_arguments,
                    maximum_instances,
                )?;
                occlusion.enqueue_build_passes(&mut queue)?;
                self.gpu_culling.enqueue_initialize_pass(&mut queue)?;
                self.gpu_culling.enqueue_hiz_cull_pass(
                    &mut queue,
                    &occlusion.hiz_resources(),
                    occlusion.hiz_views(),
                    occlusion.mip_count(),
                )?;
            } else {
                self.gpu_culling.enqueue_compute_passes(&mut queue)?;
            }
        }
        if draw_demo_scene {
            queue
                .enqueue(
                    nord_app::ApplicationRenderPassDesc::new("Nord directional shadow depth")
                        .use_resource(
                            shadow_map.color,
                            ApplicationResourceUsage::ColorAttachmentWrite,
                        )
                        .use_resource(
                            shadow_map.depth,
                            ApplicationResourceUsage::DepthAttachmentWrite,
                        )
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
        }
        let mut pbr_pass =
            nord_app::ApplicationRenderPassDesc::new("Nord metallic-roughness PBR mesh")
                .use_resource(surface.scene_color, ApplicationResourceUsage::ColorAttachmentWrite)
                .use_resource(surface.depth, ApplicationResourceUsage::DepthAttachmentWrite)
                .use_resource(camera_buffer, ApplicationResourceUsage::ConstantBufferRead)
                .use_resource(material_constants, ApplicationResourceUsage::ConstantBufferRead)
                .use_resource(material_textures[0], ApplicationResourceUsage::SampledRead)
                .use_resource(material_textures[1], ApplicationResourceUsage::SampledRead)
                .use_resource(material_textures[2], ApplicationResourceUsage::SampledRead)
                .use_resource(shadow_map.color, ApplicationResourceUsage::SampledRead)
                .use_resource(sky_buffer, ApplicationResourceUsage::VertexBufferRead)
                .use_resource(voxel_instance_buffer, ApplicationResourceUsage::VertexBufferRead);
        if draw_demo_scene {
            pbr_pass = pbr_pass
                .use_resource(vertex_buffer, ApplicationResourceUsage::VertexBufferRead)
                .use_resource(index_buffer, ApplicationResourceUsage::IndexBufferRead)
                .use_resource(visible_instances, ApplicationResourceUsage::VertexBufferRead)
                .use_resource(indirect_arguments, ApplicationResourceUsage::IndirectArgumentRead);
        }
        let pbr_pass = voxel::declare_draw_resources(pbr_pass, &voxel_draws, &self.block_textures);
        queue
            .enqueue(pbr_pass, move |encoder| {
                encoder.begin_graphics_pass_with_attachments(
                    &[ApplicationColorAttachmentBinding {
                        resource: surface.scene_color,
                        view: surface.scene_color_view,
                        clear_color: CLEAR_COLOR,
                    }],
                    Some((surface.depth, surface.depth_view)),
                    1.0,
                    Some(ApplicationViewport::new(0.0, 0.0, viewport_width, viewport_height)),
                    None,
                )?;
                encoder.draw(
                    &sky_pipeline,
                    &sky_vertex_shader,
                    &sky_pixel_shader,
                    &[],
                    &[ApplicationVertexBufferBinding {
                        slot: 0,
                        buffer: sky_buffer,
                        offset: 0,
                        size: 120,
                        stride: 20,
                    }],
                    None,
                    ApplicationDrawCall::Vertices {
                        vertex_count: 6,
                        instance_count: 1,
                        first_vertex: 0,
                        first_instance: 0,
                    },
                )?;
                if draw_demo_scene {
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
                }
                for draw in voxel_draws.iter().copied() {
                    let mut bindings = material_bindings;
                    bindings[1].descriptor = draw.base_color_view;
                    encoder.draw(
                        &pipeline,
                        &vertex_shader,
                        &pixel_shader,
                        &[
                            camera_binding,
                            bindings[0],
                            bindings[1],
                            bindings[2],
                            bindings[3],
                            bindings[4],
                            shadow_binding,
                        ],
                        &[
                            ApplicationVertexBufferBinding {
                                slot: 0,
                                buffer: draw.vertex,
                                offset: 0,
                                size: draw.vertex_size,
                                stride: VERTEX_STRIDE,
                            },
                            ApplicationVertexBufferBinding {
                                slot: 1,
                                buffer: voxel_instance_buffer,
                                offset: 0,
                                size: voxel_instance_size,
                                stride: INSTANCE_STRIDE,
                            },
                        ],
                        Some(nord_app::ApplicationIndexBufferBinding {
                            buffer: draw.index,
                            offset: 0,
                            size: draw.index_size,
                            format: nord_app::ApplicationIndexFormat::Uint32,
                        }),
                        ApplicationDrawCall::Indexed {
                            index_count: draw.index_count,
                            instance_count: 1,
                            first_index: 0,
                            base_vertex: 0,
                            first_instance: 0,
                        },
                    )?;
                }
                encoder.end_graphics_pass()?;
                Ok(())
            })
            .map_err(|error| ApplicationDiagnosticError::msg(format!("{error:?}")))
            .context("encoding the game's metallic-roughness PBR mesh pass")?;
        let scene_color_binding = ApplicationPipelineDescriptorBinding {
            kind: ApplicationShaderBindingKind::SampledResource,
            register: 0,
            space: 0,
            descriptor: surface.scene_color_srv,
        };
        queue
            .enqueue(
                nord_app::ApplicationRenderPassDesc::new(
                    "Nord scene composition and adaptive crosshair",
                )
                .use_resource(surface.scene_color, ApplicationResourceUsage::SampledRead)
                .use_resource(surface.color, ApplicationResourceUsage::ColorAttachmentWrite)
                .use_resource(crosshair_buffer, ApplicationResourceUsage::VertexBufferRead)
                .use_resource(fps_vertex_buffer, ApplicationResourceUsage::VertexBufferRead),
                move |encoder| {
                    encoder.begin_graphics_pass_with_attachments(
                        &[ApplicationColorAttachmentBinding {
                            resource: surface.color,
                            view: surface.color_view,
                            clear_color: CLEAR_COLOR,
                        }],
                        None,
                        1.0,
                        Some(ApplicationViewport::new(0.0, 0.0, viewport_width, viewport_height)),
                        None,
                    )?;
                    encoder.draw(
                        &crosshair_blit_pipeline,
                        &crosshair_blit_vertex_shader,
                        &crosshair_blit_pixel_shader,
                        &[scene_color_binding],
                        &[],
                        None,
                        ApplicationDrawCall::Vertices {
                            vertex_count: 3,
                            instance_count: 1,
                            first_vertex: 0,
                            first_instance: 0,
                        },
                    )?;
                    encoder.draw(
                        &crosshair_pipeline,
                        &crosshair_vertex_shader,
                        &crosshair_pixel_shader,
                        &[scene_color_binding],
                        &[ApplicationVertexBufferBinding {
                            slot: 0,
                            buffer: crosshair_buffer,
                            offset: 0,
                            size: 24 * 8,
                            stride: 8,
                        }],
                        None,
                        ApplicationDrawCall::Vertices {
                            vertex_count: 24,
                            instance_count: 1,
                            first_vertex: 0,
                            first_instance: 0,
                        },
                    )?;
                    encoder.draw(
                        &fps_pipeline,
                        &fps_vertex_shader,
                        &fps_pixel_shader,
                        &[],
                        &[ApplicationVertexBufferBinding {
                            slot: 0,
                            buffer: fps_vertex_buffer,
                            offset: 0,
                            size: fps_vertex_count * 24,
                            stride: 24,
                        }],
                        None,
                        ApplicationDrawCall::Vertices {
                            vertex_count: fps_vertex_count,
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
            .context("encoding the game's sampled-color crosshair composite")?;
        let graph_start = benchmark_start.map(|_| std::time::Instant::now());
        renderer
            .submit_render_queue(&mut queue, &self.jobs)
            .map_err(|error| ApplicationDiagnosticError::msg(format!("{error:?}")))
            .context("submitting the game's PBR mesh render graph")?;
        let graph_ns = graph_start.map_or(0, |start| start.elapsed().as_nanos());
        if self.gpu_validation_pending {
            let culling_stats = self.gpu_culling.validate_gpu_output(renderer)?;
            nord_app::info!(
                name: "gpu_driven_culling_validated",
                target: "nord::render",
                input_instances = self.cpu_culling_baseline.total_instances,
                visible_instances = culling_stats.visible,
                frustum_visible_instances = culling_stats.frustum_visible,
                occlusion_culled_instances = culling_stats.occlusion_culled,
                culled_instances = self
                    .cpu_culling_baseline
                    .total_instances
                    .saturating_sub(culling_stats.visible),
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
        let present_start = benchmark_start.map(|_| std::time::Instant::now());
        renderer
            .present_texture(surface.color)
            .map_err(ApplicationDiagnosticError::msg)
            .context("presenting the game's 3D mesh")?;
        let present_ns = present_start.map_or(0, |start| start.elapsed().as_nanos());
        if let Some(benchmark_start) = benchmark_start {
            collect_render_benchmark(
                renderer,
                surface,
                self.occlusion.is_some(),
                &mut self.benchmark_samples,
                &mut self.benchmark_pending,
                BenchmarkFrame {
                    frame_ns: benchmark_start.elapsed().as_nanos(),
                    graph_ns,
                    present_ns,
                    stream_ns,
                    mesh_sync_ns,
                    queue: renderer.last_queue_timing(),
                },
                visible_voxel_draws,
            )?;
        }
        Ok(())
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

    pub(super) fn handle_cursor_capture_failure(&mut self) {
        self.camera.handle_cursor_capture_failure();
    }

    pub(super) fn advance_simulation(
        &mut self,
        inputs: &nord_app::ApplicationInputSystem,
        fixed_step: std::time::Duration,
    ) -> nord_app::ApplicationResult<()> {
        self.camera.advance_fixed_step(inputs, fixed_step)
    }

    pub(super) fn resize(&mut self, width: u32, height: u32) -> nord_app::ApplicationResult<()> {
        let renderer = self
            .renderer
            .as_mut()
            .ok_or_else(|| ApplicationDiagnosticError::msg("renderer has already shut down"))?;
        match surface_resize_action(
            self.surface.width,
            self.surface.height,
            width,
            height,
            renderer.surface_suspended(),
        ) {
            SurfaceResizeAction::Suspend => {
                return renderer
                    .resize(width, height)
                    .map_err(ApplicationDiagnosticError::msg)
                    .context("suspending the Nord render surface");
            }
            SurfaceResizeAction::Resume => {
                return renderer
                    .resize(width, height)
                    .map_err(ApplicationDiagnosticError::msg)
                    .context("resuming the Nord render surface");
            }
            SurfaceResizeAction::Unchanged => return Ok(()),
            SurfaceResizeAction::Recreate => {}
        }
        let aspect_ratio = surface_aspect(width, height)?;
        if let Some(occlusion) = self.occlusion.take() {
            occlusion.shutdown(renderer)?;
        }
        renderer
            .destroy_resource(self.surface.scene_color)
            .map_err(ApplicationDiagnosticError::msg)
            .context("retiring the previous sampled scene color target")?;
        renderer
            .destroy_resource(self.surface.color)
            .map_err(ApplicationDiagnosticError::msg)
            .context("retiring the previous composited output target")?;
        renderer
            .destroy_resource(self.surface.depth)
            .map_err(ApplicationDiagnosticError::msg)
            .context("retiring the previous game depth target")?;
        renderer
            .resize(width, height)
            .map_err(ApplicationDiagnosticError::msg)
            .context("resizing the Nord render surface")?;
        self.surface = Self::create_surface(renderer, width, height)?;
        self.crosshair.resize(renderer, width, height)?;
        if occlusion_culling_enabled() {
            self.occlusion = Some(HiZOcclusion::create(renderer, width, height)?);
        }
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
        self.chunk_streamer.cancel_all();
        self.scheduler
            .shutdown(ApplicationShutdownMode::CancelPending)
            .map_err(ApplicationDiagnosticError::msg)
            .context("cancelling pending chunk work before game shutdown")?;
        let Some(mut renderer) = self.renderer.take() else {
            return Ok(());
        };
        renderer
            .wait_idle()
            .map_err(ApplicationDiagnosticError::msg)
            .context("waiting for graphics work before game renderer shutdown")?;
        self.voxel_renderer.shutdown(&mut renderer)?;
        self.block_textures.shutdown(&mut renderer)?;
        self.material.shutdown(&mut renderer)?;
        self.shadows.shutdown(&mut renderer)?;
        if let Some(occlusion) = &self.occlusion {
            occlusion.shutdown(&mut renderer)?;
        }
        self.gpu_culling.destroy(&mut renderer)?;
        for resource in [
            self.surface.scene_color,
            self.surface.color,
            self.surface.depth,
            self.vertex_buffer,
            self.index_buffer,
            self.instance_buffer,
            self.camera_buffer,
            self.fps_hud.vertex_buffer(),
        ] {
            renderer
                .destroy_resource(resource)
                .map_err(ApplicationDiagnosticError::msg)
                .context("destroying a game-owned 3D render resource")?;
        }
        Ok(())
    }
}

fn floor_block_coordinate(value: f32) -> nord_app::ApplicationResult<i32> {
    if !value.is_finite() {
        return Err(ApplicationDiagnosticError::msg(
            "camera position cannot be converted to a block coordinate",
        ));
    }
    let floored = f64::from(value).floor();
    if floored < f64::from(i32::MIN) || floored > f64::from(i32::MAX) {
        return Err(ApplicationDiagnosticError::msg(
            "camera is outside the block coordinate range",
        ));
    }
    #[allow(clippy::cast_possible_truncation)] // The finite range check above makes this exact.
    Ok(floored as i32)
}

fn collect_render_benchmark(
    renderer: &mut ApplicationRenderer,
    surface: GameSurface,
    hiz_enabled: bool,
    samples: &mut Vec<BenchmarkFrame>,
    pending: &mut bool,
    frame: BenchmarkFrame,
    visible_voxel_draws: usize,
) -> nord_app::ApplicationResult<()> {
    samples.push(frame);
    if samples.len() < RENDER_BENCHMARK_WARMUP_FRAMES + RENDER_BENCHMARK_MEASURED_FRAMES {
        return Ok(());
    }
    let measured = &samples[RENDER_BENCHMARK_WARMUP_FRAMES..];
    let percentile = |percent: usize, select: fn(&BenchmarkFrame) -> u128| -> u128 {
        let mut values = measured.iter().map(select).collect::<Vec<_>>();
        values.sort_unstable();
        let rank = values.len().saturating_mul(percent).div_ceil(100).max(1);
        values[rank - 1].max(1)
    };
    let p50_ns = percentile(50, |sample| sample.frame_ns);
    let p95_ns = percentile(95, |sample| sample.frame_ns);
    let p50_graph_ns = percentile(50, |sample| sample.graph_ns);
    let p95_graph_ns = percentile(95, |sample| sample.graph_ns);
    let p50_present_ns = percentile(50, |sample| sample.present_ns);
    let p95_present_ns = percentile(95, |sample| sample.present_ns);
    let p50_stream_ns = percentile(50, |sample| sample.stream_ns);
    let p95_stream_ns = percentile(95, |sample| sample.stream_ns);
    let p50_mesh_sync_ns = percentile(50, |sample| sample.mesh_sync_ns);
    let p95_mesh_sync_ns = percentile(95, |sample| sample.mesh_sync_ns);
    let p50_fps = 1_000_000_000.0 / p50_ns as f64;
    let p95_fps = 1_000_000_000.0 / p95_ns as f64;
    let (width, height, pixels) = renderer
        .read_texture_rgba8(surface.scene_color)
        .map_err(ApplicationDiagnosticError::msg)
        .context("reading the completed frame for deterministic visual comparison")?;
    let checksum = pixels.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    let gpu_frames = renderer
        .take_gpu_frame_timings()
        .map_err(ApplicationDiagnosticError::msg)
        .context("collecting completed direct-queue GPU timestamps")?;
    let gpu_measured = gpu_frames
        .get(gpu_frames.len().saturating_sub(RENDER_BENCHMARK_MEASURED_FRAMES)..)
        .unwrap_or(&[]);
    let gpu_report = gpu_timing_report(gpu_measured);
    let cpu_stages = cpu_queue_timing_report(measured);
    nord_app::info!(
        name: "render_path_comparison",
        target: "nord::render",
        path = if hiz_enabled { "same-frame-hiz" } else { "frustum-only" },
        warmup_frames = RENDER_BENCHMARK_WARMUP_FRAMES,
        measured_frames = RENDER_BENCHMARK_MEASURED_FRAMES,
        p50_complete_frame_ns = p50_ns,
        p95_complete_frame_ns = p95_ns,
        p50_fps,
        p95_fps,
        width,
        height,
        visible_voxel_draws,
        visual_checksum = format!("{checksum:016x}"),
        "Render-path timing and output checksum captured for A/B comparison"
    );
    let report = format!(
        "{{\n  \"measurement\": \"steady_state_render_frame_cpu_and_direct_queue_gpu\",\n  \"vsync\": false,\n  \"gpu_drain_after_measurement\": true,\n  \"warmup_frames\": {RENDER_BENCHMARK_WARMUP_FRAMES},\n  \"measured_frames\": {RENDER_BENCHMARK_MEASURED_FRAMES},\n  \"p50_frame_ns\": {p50_ns},\n  \"p95_frame_ns\": {p95_ns},\n  \"p50_stream_ns\": {p50_stream_ns},\n  \"p95_stream_ns\": {p95_stream_ns},\n  \"p50_mesh_sync_ns\": {p50_mesh_sync_ns},\n  \"p95_mesh_sync_ns\": {p95_mesh_sync_ns},\n  \"p50_graph_ns\": {p50_graph_ns},\n  \"p95_graph_ns\": {p95_graph_ns},\n  \"p50_present_ns\": {p50_present_ns},\n  \"p95_present_ns\": {p95_present_ns},\n  \"p50_fps\": {p50_fps:.2},\n  \"p95_fps\": {p95_fps:.2},\n  \"width\": {width},\n  \"height\": {height},\n  \"visible_voxel_draws\": {visible_voxel_draws},\n  \"checksum_surface\": \"scene_color\",\n  \"visual_checksum\": \"{checksum:016x}\",\n  \"cpu_queue_stages\": {cpu_stages},\n  \"gpu\": {gpu_report}\n}}\n"
    );
    let output_path = std::env::current_exe().ok().and_then(|executable| {
        executable.parent().map(|parent| parent.join("nord-render-benchmark.json"))
    });
    let output_result =
        output_path.map(|path| std::fs::write(&path, report).map(|()| path)).transpose();
    match output_result {
        Ok(Some(path)) => nord_app::info!(
            target: "nord::render",
            benchmark_report = %path.display(),
            "Wrote the render benchmark report"
        ),
        Ok(None) => nord_app::warn!(
            target: "nord::render",
            "Could not locate the executable path for the render benchmark report"
        ),
        Err(error) => nord_app::warn!(
            target: "nord::render",
            %error,
            "Could not write the render benchmark report beside the executable"
        ),
    }
    *pending = false;
    Ok(())
}

fn cpu_queue_timing_report(frames: &[BenchmarkFrame]) -> String {
    let stages: [(&str, fn(&ApplicationRenderQueueTiming) -> u64); 6] = [
        ("encode", |timing| timing.encode_ns),
        ("transients", |timing| timing.transients_ns),
        ("barriers", |timing| timing.barriers_ns),
        ("commands", |timing| timing.commands_ns),
        ("submit", |timing| timing.submit_ns),
        ("cleanup", |timing| timing.cleanup_ns),
    ];
    let mut result = serde_json::Map::new();
    for (name, select) in stages {
        let mut values = frames.iter().map(|frame| select(&frame.queue)).collect::<Vec<_>>();
        values.sort_unstable();
        let p50 = values[values.len().saturating_mul(50).div_ceil(100).max(1) - 1];
        let p95 = values[values.len().saturating_mul(95).div_ceil(100).max(1) - 1];
        result.insert(name.to_owned(), serde_json::json!({"p50_ns": p50, "p95_ns": p95}));
    }
    serde_json::Value::Object(result).to_string()
}

fn gpu_timing_report(frames: &[ApplicationGpuFrameTiming]) -> String {
    if frames.is_empty() {
        return "null".to_string();
    }
    let percentile = |values: &mut Vec<u64>, percent: usize| {
        values.sort_unstable();
        let rank = values.len().saturating_mul(percent).div_ceil(100).max(1);
        values[rank - 1]
    };
    let mut graph = frames.iter().map(|frame| frame.graph_nanoseconds).collect::<Vec<_>>();
    let mut copy = frames.iter().map(|frame| frame.present_copy_nanoseconds).collect::<Vec<_>>();
    let mut gap =
        frames.iter().map(|frame| frame.graph_to_copy_gap_nanoseconds).collect::<Vec<_>>();
    let pass_count = frames[0].passes.len();
    let passes = (0..pass_count)
        .map(|index| {
            let mut values = frames.iter().map(|frame| frame.passes[index].nanoseconds).collect();
            let p50 = percentile(&mut values, 50);
            let p95 = percentile(&mut values, 95);
            serde_json::json!({"name": frames[0].passes[index].name, "p50_ns": p50, "p95_ns": p95})
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "sample_count": frames.len(),
        "graph_p50_ns": percentile(&mut graph, 50),
        "graph_p95_ns": percentile(&mut graph, 95),
        "present_copy_p50_ns": percentile(&mut copy, 50),
        "present_copy_p95_ns": percentile(&mut copy, 95),
        "graph_to_copy_gap_p50_ns": percentile(&mut gap, 50),
        "graph_to_copy_gap_p95_ns": percentile(&mut gap, 95),
        "passes": passes,
    })
    .to_string()
}

fn occlusion_culling_enabled() -> bool {
    std::env::var("NORD_OCCLUSION_CULLING")
        .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
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

fn vector_capacity_bytes<T>(capacity: usize) -> nord_app::ApplicationResult<u64> {
    let bytes = capacity
        .checked_mul(std::mem::size_of::<T>())
        .ok_or_else(|| ApplicationDiagnosticError::msg("vector memory accounting overflowed"))?;
    u64::try_from(bytes).map_err(ApplicationDiagnosticError::msg)
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

#[cfg(test)]
mod tests {
    use super::{SurfaceResizeAction, surface_resize_action};

    #[test]
    fn minimized_surface_resumes_even_when_restored_to_the_same_extent() {
        assert_eq!(surface_resize_action(1280, 720, 0, 0, false), SurfaceResizeAction::Suspend);
        assert_eq!(surface_resize_action(1280, 720, 1280, 720, true), SurfaceResizeAction::Resume);
    }

    #[test]
    fn resize_policy_distinguishes_noop_and_surface_recreation() {
        assert_eq!(
            surface_resize_action(1280, 720, 1280, 720, false),
            SurfaceResizeAction::Unchanged
        );
        assert_eq!(
            surface_resize_action(1280, 720, 1600, 900, false),
            SurfaceResizeAction::Recreate
        );
    }
}
