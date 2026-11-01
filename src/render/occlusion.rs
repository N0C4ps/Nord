//! Same-frame camera depth and conservative maximum-reduction Hi-Z policy.

use nord_app::{
    ApplicationColorAttachmentBinding, ApplicationCompiledShader, ApplicationComputePipelineDesc,
    ApplicationDescriptorHandle, ApplicationDiagnosticError, ApplicationErrorContext,
    ApplicationGpuResourceHandle, ApplicationGraphicsPipelineDesc, ApplicationIndexBufferBinding,
    ApplicationIndexFormat, ApplicationPipelineDescriptorBinding, ApplicationRenderPassDesc,
    ApplicationRenderQueue, ApplicationRenderer, ApplicationResourceUsage,
    ApplicationShaderBindingKind, ApplicationShaderStage, ApplicationTextureDesc,
    ApplicationTextureFormat, ApplicationTextureUsage, ApplicationVertexAttribute,
    ApplicationVertexBufferBinding, ApplicationVertexFormat, ApplicationViewport,
};

const HIZ_LEVEL_COUNT: usize = 15;
const THREADS_PER_GROUP: u32 = 8;

pub(super) struct HiZOcclusion {
    depth_color: ApplicationGpuResourceHandle,
    depth_color_view: ApplicationDescriptorHandle,
    depth_attachment: ApplicationGpuResourceHandle,
    depth_attachment_view: ApplicationDescriptorHandle,
    levels: Vec<ApplicationGpuResourceHandle>,
    level_views: Vec<ApplicationDescriptorHandle>,
    level_uavs: Vec<ApplicationDescriptorHandle>,
    width: u32,
    height: u32,
    depth_pipeline: ApplicationGraphicsPipelineDesc,
    depth_vertex: ApplicationCompiledShader,
    depth_pixel: ApplicationCompiledShader,
    build_pipeline: ApplicationComputePipelineDesc,
    build_shader: ApplicationCompiledShader,
}

impl HiZOcclusion {
    #[allow(clippy::too_many_lines)]
    pub(super) fn create(
        renderer: &mut ApplicationRenderer,
        width: u32,
        height: u32,
    ) -> nord_app::ApplicationResult<Self> {
        let target = renderer
            .create_render_target(nord_app::ApplicationRenderTargetDesc::new(
                width,
                height,
                Some(ApplicationTextureFormat::R32Float),
                Some(ApplicationTextureFormat::D32Float),
            )?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating current-frame occlusion depth targets")?;
        let depth_color = target.color.expect("requested the camera depth color target");
        let depth_attachment = target.depth.expect("requested the camera D32 attachment");
        let depth_color_view = renderer
            .create_render_target_view(depth_color)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the camera depth color attachment")?;
        let depth_attachment_view = renderer
            .create_depth_stencil_view(depth_attachment)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the camera depth test attachment")?;

        let mut levels = Vec::with_capacity(HIZ_LEVEL_COUNT);
        let mut current_width = width;
        let mut current_height = height;
        for _ in 1..HIZ_LEVEL_COUNT {
            current_width = current_width.div_ceil(2).max(1);
            current_height = current_height.div_ceil(2).max(1);
            levels.push(
                renderer
                    .create_texture(ApplicationTextureDesc::new(
                        current_width,
                        current_height,
                        ApplicationTextureFormat::R32Float,
                        ApplicationTextureUsage::SAMPLED.union(ApplicationTextureUsage::STORAGE),
                    )?)
                    .map_err(ApplicationDiagnosticError::msg)
                    .context("allocating a conservative Hi-Z reduction level")?,
            );
        }
        let mut resources = Vec::with_capacity(HIZ_LEVEL_COUNT);
        resources.push(depth_color);
        resources.extend(levels.iter().copied());
        let level_views = renderer
            .create_texture_srv_array(&resources)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating contiguous SRVs for the Hi-Z depth pyramid")?;
        let level_uavs = renderer
            .create_unordered_access_view_array(&levels)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating UAVs for the Hi-Z depth pyramid")?;

        let depth_vertex = load_shader(
            ApplicationShaderStage::Vertex,
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_camera_depth.vs.dxil")),
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_camera_depth.vs.reflection")),
            include_str!(concat!(env!("OUT_DIR"), "/nord_camera_depth.vs.asm")),
        )?;
        let depth_pixel = load_shader(
            ApplicationShaderStage::Pixel,
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_camera_depth.ps.dxil")),
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_camera_depth.ps.reflection")),
            include_str!(concat!(env!("OUT_DIR"), "/nord_camera_depth.ps.asm")),
        )?;
        let mut depth_pipeline = ApplicationGraphicsPipelineDesc::new(
            depth_vertex.shader_id,
            depth_pixel.shader_id,
            ApplicationTextureFormat::R32Float,
        );
        depth_pipeline.depth_format = Some(ApplicationTextureFormat::D32Float);
        depth_pipeline.depth_test = true;
        depth_pipeline.depth_write = true;
        depth_pipeline.alpha_blending = false;
        depth_pipeline = depth_pipeline
            .with_vertex_attribute(vertex_attribute(
                "POSITION",
                ApplicationVertexFormat::Float3,
                0,
                false,
            ))
            .with_vertex_attribute(vertex_attribute(
                "INSTANCE_TRANSLATION",
                ApplicationVertexFormat::Float3,
                0,
                true,
            ));
        renderer
            .create_graphics_pipeline(&depth_pipeline, &depth_vertex, &depth_pixel, 1)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the current-camera depth prepass pipeline")?;

        let build_shader = load_shader(
            ApplicationShaderStage::Compute,
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_hiz.cs.dxil")),
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_hiz.cs.reflection")),
            include_str!(concat!(env!("OUT_DIR"), "/nord_hiz.cs.asm")),
        )?;
        let build_pipeline =
            ApplicationComputePipelineDesc { compute_shader: build_shader.shader_id };
        renderer
            .create_compute_pipeline(build_pipeline, &build_shader, 1)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the Hi-Z maximum-reduction pipeline")?;

        Ok(Self {
            depth_color,
            depth_color_view,
            depth_attachment,
            depth_attachment_view,
            levels,
            level_views,
            level_uavs,
            width,
            height,
            depth_pipeline,
            depth_vertex,
            depth_pixel,
            build_pipeline,
            build_shader,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn enqueue_depth_prepass(
        &self,
        queue: &mut ApplicationRenderQueue,
        camera_buffer: ApplicationGpuResourceHandle,
        camera_binding: ApplicationPipelineDescriptorBinding,
        vertex_buffer: ApplicationGpuResourceHandle,
        vertex_buffer_size: u32,
        index_buffer: ApplicationGpuResourceHandle,
        index_buffer_size: u32,
        index_count: u32,
        visible_instances: ApplicationGpuResourceHandle,
        visible_instances_size: u32,
        indirect_arguments: ApplicationGpuResourceHandle,
        maximum_instances: u32,
    ) -> nord_app::ApplicationResult<()> {
        let depth_color = self.depth_color;
        let depth_color_view = self.depth_color_view;
        let depth_attachment = self.depth_attachment;
        let depth_attachment_view = self.depth_attachment_view;
        let width = self.width;
        let height = self.height;
        let viewport_width = super::exact_surface_extent(width)?;
        let viewport_height = super::exact_surface_extent(height)?;
        let pipeline = self.depth_pipeline.clone();
        let vertex = self.depth_vertex.clone();
        let pixel = self.depth_pixel.clone();
        queue
            .enqueue(
                ApplicationRenderPassDesc::new("Nord camera depth prepass for Hi-Z")
                    .use_resource(depth_color, ApplicationResourceUsage::ColorAttachmentWrite)
                    .use_resource(depth_attachment, ApplicationResourceUsage::DepthAttachmentWrite)
                    .use_resource(vertex_buffer, ApplicationResourceUsage::VertexBufferRead)
                    .use_resource(index_buffer, ApplicationResourceUsage::IndexBufferRead)
                    .use_resource(visible_instances, ApplicationResourceUsage::VertexBufferRead)
                    .use_resource(
                        indirect_arguments,
                        ApplicationResourceUsage::IndirectArgumentRead,
                    )
                    .use_resource(camera_buffer, ApplicationResourceUsage::ConstantBufferRead),
                move |encoder| {
                    encoder.begin_graphics_pass_with_attachments(
                        &[ApplicationColorAttachmentBinding {
                            resource: depth_color,
                            view: depth_color_view,
                            clear_color: nord_app::ApplicationClearColor::rgba(1.0, 1.0, 1.0, 1.0),
                        }],
                        Some((depth_attachment, depth_attachment_view)),
                        1.0,
                        Some(ApplicationViewport::new(0.0, 0.0, viewport_width, viewport_height)),
                        None,
                    )?;
                    encoder.draw_indexed_indirect(
                        &pipeline,
                        &vertex,
                        &pixel,
                        &[camera_binding],
                        &[
                            ApplicationVertexBufferBinding {
                                slot: 0,
                                buffer: vertex_buffer,
                                offset: 0,
                                size: vertex_buffer_size,
                                stride: 48,
                            },
                            ApplicationVertexBufferBinding {
                                slot: 1,
                                buffer: visible_instances,
                                offset: 0,
                                size: visible_instances_size,
                                stride: 24,
                            },
                        ],
                        ApplicationIndexBufferBinding {
                            buffer: index_buffer,
                            offset: 0,
                            size: index_buffer_size,
                            format: ApplicationIndexFormat::Uint16,
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
            .context("adding the current-frame camera depth prepass")?;
        Ok(())
    }

    pub(super) fn enqueue_build_passes(
        &self,
        queue: &mut ApplicationRenderQueue,
    ) -> nord_app::ApplicationResult<()> {
        let mut source = self.depth_color;
        let mut source_view = self.level_views[0];
        let mut source_width = self.width;
        let mut source_height = self.height;
        for (index, (&destination, &destination_uav)) in
            self.levels.iter().zip(&self.level_uavs).enumerate()
        {
            let destination_view = self.level_views[index + 1];
            let destination_width = source_width.div_ceil(2).max(1);
            let destination_height = source_height.div_ceil(2).max(1);
            let pipeline = self.build_pipeline;
            let shader = self.build_shader.clone();
            let bindings = [
                ApplicationPipelineDescriptorBinding {
                    kind: ApplicationShaderBindingKind::SampledResource,
                    register: 0,
                    space: 0,
                    descriptor: source_view,
                },
                ApplicationPipelineDescriptorBinding {
                    kind: ApplicationShaderBindingKind::StorageResource,
                    register: 0,
                    space: 0,
                    descriptor: destination_uav,
                },
            ];
            queue
                .enqueue(
                    ApplicationRenderPassDesc::new(format!(
                        "Nord Hi-Z reduction level {}",
                        index + 1
                    ))
                    .use_resource(source, ApplicationResourceUsage::SampledRead)
                    .use_resource(destination, ApplicationResourceUsage::StorageWrite),
                    move |encoder| {
                        encoder.dispatch(
                            pipeline,
                            &shader,
                            &bindings,
                            [
                                destination_width.div_ceil(THREADS_PER_GROUP),
                                destination_height.div_ceil(THREADS_PER_GROUP),
                                1,
                            ],
                        )?;
                        Ok(())
                    },
                )
                .map_err(|error| ApplicationDiagnosticError::msg(format!("{error:?}")))
                .context("adding a max-reduction pass to the Hi-Z pyramid")?;
            source = destination;
            source_view = destination_view;
            source_width = destination_width;
            source_height = destination_height;
        }
        Ok(())
    }

    pub(super) fn hiz_resources(&self) -> Vec<ApplicationGpuResourceHandle> {
        let mut resources = Vec::with_capacity(HIZ_LEVEL_COUNT);
        resources.push(self.depth_color);
        resources.extend(self.levels.iter().copied());
        resources
    }

    pub(super) fn hiz_views(&self) -> &[ApplicationDescriptorHandle] {
        &self.level_views
    }

    pub(super) const fn mip_count() -> u32 {
        15
    }

    pub(super) fn shutdown(
        &self,
        renderer: &mut ApplicationRenderer,
    ) -> nord_app::ApplicationResult<()> {
        for descriptor in self
            .level_views
            .iter()
            .chain(&self.level_uavs)
            .copied()
            .chain([self.depth_color_view, self.depth_attachment_view])
        {
            renderer
                .destroy_descriptor(descriptor)
                .map_err(ApplicationDiagnosticError::msg)
                .context("retiring a Hi-Z texture descriptor")?;
        }
        for resource in
            [self.depth_color, self.depth_attachment].into_iter().chain(self.levels.iter().copied())
        {
            renderer
                .destroy_resource(resource)
                .map_err(ApplicationDiagnosticError::msg)
                .context("retiring a camera depth or Hi-Z texture")?;
        }
        Ok(())
    }
}

fn load_shader(
    stage: ApplicationShaderStage,
    bytecode: &[u8],
    reflection: &[u8],
    dump: &str,
) -> nord_app::ApplicationResult<ApplicationCompiledShader> {
    ApplicationCompiledShader::from_precompiled(
        stage,
        bytecode.to_vec(),
        reflection.to_vec(),
        dump.to_owned(),
    )
    .map_err(ApplicationDiagnosticError::msg)
    .context("loading an embedded Hi-Z or camera-depth shader")
}

fn vertex_attribute(
    semantic: &str,
    format: ApplicationVertexFormat,
    byte_offset: u32,
    per_instance: bool,
) -> ApplicationVertexAttribute {
    ApplicationVertexAttribute {
        semantic: semantic.into(),
        semantic_index: 0,
        format,
        input_slot: u32::from(per_instance),
        byte_offset,
        per_instance,
        instance_step_rate: u32::from(per_instance),
    }
}
