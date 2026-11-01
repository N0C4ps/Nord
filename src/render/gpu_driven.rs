//! Game-owned GPU culling policy and its renderer resource bindings.

use nord_app::{
    ApplicationBufferDesc, ApplicationBufferMemory, ApplicationBufferUsage,
    ApplicationCompiledShader, ApplicationComputePipelineDesc, ApplicationDescriptorHandle,
    ApplicationDiagnosticError, ApplicationErrorContext, ApplicationGpuResourceHandle,
    ApplicationPipelineDescriptorBinding, ApplicationPipelineDescriptorTableBinding,
    ApplicationRenderPassDesc, ApplicationRenderQueue, ApplicationRenderer,
    ApplicationResourceUsage, ApplicationShaderBindingKind, ApplicationShaderStage,
};

const CONSTANT_BYTES: usize = 256;
const INDIRECT_ARGUMENT_BYTES: u64 = 20;
const INDIRECT_ARGUMENT_BYTE_COUNT: usize = 20;
const VALIDATION_READBACK_BYTES: u64 = 28;
const THREADS_PER_GROUP: u32 = 64;

pub(super) struct GpuCullingStats {
    pub(super) visible: u32,
    pub(super) frustum_visible: u32,
    pub(super) occlusion_culled: u32,
}

pub(super) struct GpuDrivenCulling {
    source_instances: ApplicationGpuResourceHandle,
    visible_instances: ApplicationGpuResourceHandle,
    visible_instances_size: u32,
    visible_uav: ApplicationDescriptorHandle,
    arguments: ApplicationGpuResourceHandle,
    arguments_uav: ApplicationDescriptorHandle,
    statistics: ApplicationGpuResourceHandle,
    statistics_uav: ApplicationDescriptorHandle,
    source_srv: ApplicationDescriptorHandle,
    constants: ApplicationGpuResourceHandle,
    initialize_pipeline: ApplicationComputePipelineDesc,
    initialize_shader: ApplicationCompiledShader,
    initialize_bindings: [ApplicationPipelineDescriptorBinding; 3],
    cull_pipeline: ApplicationComputePipelineDesc,
    cull_shader: ApplicationCompiledShader,
    cull_bindings: [ApplicationPipelineDescriptorBinding; 5],
    hiz_pipeline: ApplicationComputePipelineDesc,
    hiz_shader: ApplicationCompiledShader,
    hiz_bindings: [ApplicationPipelineDescriptorBinding; 5],
    instance_count: u32,
    index_count: u32,
    bounds_radius: f32,
    bounds_half_extents: nord_app::ApplicationVec3,
}

impl GpuDrivenCulling {
    #[allow(clippy::too_many_lines)]
    pub(super) fn create(
        renderer: &mut ApplicationRenderer,
        source_instances: ApplicationGpuResourceHandle,
        instance_bytes: u32,
        instance_count: u32,
        index_count: u32,
        bounds_half_extents: nord_app::ApplicationVec3,
    ) -> nord_app::ApplicationResult<Self> {
        if instance_bytes == 0
            || !instance_bytes.is_multiple_of(24)
            || instance_count == 0
            || index_count == 0
            || !bounds_half_extents.x.is_finite()
            || !bounds_half_extents.y.is_finite()
            || !bounds_half_extents.z.is_finite()
            || bounds_half_extents.x <= 0.0
            || bounds_half_extents.y <= 0.0
            || bounds_half_extents.z <= 0.0
        {
            return Err(ApplicationDiagnosticError::msg(
                "GPU culling requires non-empty, stride-aligned instances and indexed geometry",
            ));
        }
        let visible_instances = renderer
            .create_buffer(ApplicationBufferDesc::new(
                u64::from(instance_bytes),
                ApplicationBufferUsage::STORAGE.union(ApplicationBufferUsage::VERTEX),
            )?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating compacted GPU-visible instance storage")?;
        let arguments = renderer
            .create_buffer(ApplicationBufferDesc::new(
                INDIRECT_ARGUMENT_BYTES,
                ApplicationBufferUsage::STORAGE
                    .union(ApplicationBufferUsage::INDIRECT_ARGUMENT)
                    .union(ApplicationBufferUsage::COPY_SOURCE),
            )?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating the GPU indexed-indirect argument buffer")?;
        let statistics = renderer
            .create_buffer(ApplicationBufferDesc::new(
                8,
                ApplicationBufferUsage::STORAGE.union(ApplicationBufferUsage::COPY_SOURCE),
            )?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating GPU culling diagnostic counters")?;
        let constants = renderer
            .create_buffer(
                ApplicationBufferDesc::new(
                    CONSTANT_BYTES as u64,
                    ApplicationBufferUsage::CONSTANT,
                )?
                .with_memory(ApplicationBufferMemory::Upload),
            )
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating per-frame GPU-culling constants")?;
        let source_srv = renderer
            .create_buffer_srv(source_instances)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the GPU-culling source instance SRV")?;
        let visible_uav = renderer
            .create_unordered_access_view(visible_instances)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating compacted instance storage UAV")?;
        let arguments_uav = renderer
            .create_unordered_access_view(arguments)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating indexed-indirect argument UAV")?;
        let statistics_uav = renderer
            .create_unordered_access_view(statistics)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating GPU culling statistics UAV")?;
        let constant_view = renderer
            .create_constant_buffer_view(constants)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating GPU-culling constant-buffer binding")?;
        let initialize_shader = load_shader(
            ApplicationShaderStage::Compute,
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_gpu_culling_init.cs.dxil")),
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_gpu_culling_init.cs.reflection")),
            include_str!(concat!(env!("OUT_DIR"), "/nord_gpu_culling_init.cs.asm")),
        )?;
        let cull_shader = load_shader(
            ApplicationShaderStage::Compute,
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_gpu_culling_cull.cs.dxil")),
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_gpu_culling_cull.cs.reflection")),
            include_str!(concat!(env!("OUT_DIR"), "/nord_gpu_culling_cull.cs.asm")),
        )?;
        let hiz_shader = load_shader(
            ApplicationShaderStage::Compute,
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_gpu_culling_hiz.cs.dxil")),
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_gpu_culling_hiz.cs.reflection")),
            include_str!(concat!(env!("OUT_DIR"), "/nord_gpu_culling_hiz.cs.asm")),
        )?;
        let initialize_pipeline =
            ApplicationComputePipelineDesc { compute_shader: initialize_shader.shader_id };
        let cull_pipeline =
            ApplicationComputePipelineDesc { compute_shader: cull_shader.shader_id };
        let hiz_pipeline = ApplicationComputePipelineDesc { compute_shader: hiz_shader.shader_id };
        renderer
            .create_compute_pipeline(initialize_pipeline, &initialize_shader, 0)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the indirect-argument initialization compute pipeline")?;
        renderer
            .create_compute_pipeline(cull_pipeline, &cull_shader, 0)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the GPU instance-culling compute pipeline")?;
        renderer
            .create_compute_pipeline(hiz_pipeline, &hiz_shader, 0)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating conservative Hi-Z instance-culling compute pipeline")?;
        let initialize_bindings = [
            binding(ApplicationShaderBindingKind::ConstantBuffer, 0, constant_view),
            binding(ApplicationShaderBindingKind::StorageResource, 1, arguments_uav),
            binding(ApplicationShaderBindingKind::StorageResource, 2, statistics_uav),
        ];
        let cull_bindings = [
            binding(ApplicationShaderBindingKind::ConstantBuffer, 0, constant_view),
            binding(ApplicationShaderBindingKind::SampledResource, 0, source_srv),
            binding(ApplicationShaderBindingKind::StorageResource, 0, visible_uav),
            binding(ApplicationShaderBindingKind::StorageResource, 1, arguments_uav),
            binding(ApplicationShaderBindingKind::StorageResource, 2, statistics_uav),
        ];
        let hiz_bindings = [
            binding(ApplicationShaderBindingKind::ConstantBuffer, 0, constant_view),
            binding(ApplicationShaderBindingKind::SampledResource, 0, source_srv),
            binding(ApplicationShaderBindingKind::StorageResource, 0, visible_uav),
            binding(ApplicationShaderBindingKind::StorageResource, 1, arguments_uav),
            binding(ApplicationShaderBindingKind::StorageResource, 2, statistics_uav),
        ];
        Ok(Self {
            source_instances,
            visible_instances,
            visible_instances_size: instance_bytes,
            visible_uav,
            arguments,
            arguments_uav,
            statistics,
            statistics_uav,
            source_srv,
            constants,
            initialize_pipeline,
            initialize_shader,
            initialize_bindings,
            cull_pipeline,
            cull_shader,
            cull_bindings,
            hiz_pipeline,
            hiz_shader,
            hiz_bindings,
            instance_count,
            index_count,
            bounds_radius: bounds_half_extents.length(),
            bounds_half_extents,
        })
    }

    pub(super) fn update_constants(
        &self,
        renderer: &mut ApplicationRenderer,
        planes: [nord_app::ApplicationPlane3; 6],
        view_projection: nord_app::ApplicationMat4,
        hiz_mip_count: u32,
    ) -> nord_app::ApplicationResult<()> {
        let mut bytes = [0_u8; CONSTANT_BYTES];
        for (index, plane) in planes.into_iter().enumerate() {
            let normal = plane.normal();
            let offset = index * 16;
            for (component, value) in
                [normal.x, normal.y, normal.z, plane.distance()].into_iter().enumerate()
            {
                bytes[offset + component * 4..offset + component * 4 + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
        write_u32(&mut bytes, 96, self.instance_count);
        write_u32(&mut bytes, 100, self.index_count);
        bytes[104..108].copy_from_slice(&self.bounds_radius.to_le_bytes());
        write_u32(&mut bytes, 108, hiz_mip_count);
        for (column, values) in view_projection.to_cols().into_iter().enumerate() {
            for (row, value) in values.to_array().into_iter().enumerate() {
                let offset = 112 + column * 16 + row * 4;
                bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        for (component, value) in
            [self.bounds_half_extents.x, self.bounds_half_extents.y, self.bounds_half_extents.z]
                .into_iter()
                .enumerate()
        {
            let offset = 176 + component * 4;
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        renderer
            .write_buffer(self.constants, 0, &bytes)
            .map_err(ApplicationDiagnosticError::msg)
            .context("updating frustum planes and instance counts for GPU culling")
    }

    pub(super) fn enqueue_compute_passes(
        &self,
        queue: &mut ApplicationRenderQueue,
    ) -> nord_app::ApplicationResult<()> {
        let arguments = self.arguments;
        let initialize_pipeline = self.initialize_pipeline;
        let initialize_shader = self.initialize_shader.clone();
        let initialize_bindings = self.initialize_bindings;
        queue
            .enqueue(
                ApplicationRenderPassDesc::new("Nord GPU indirect argument initialization")
                    .use_resource(self.constants, ApplicationResourceUsage::ConstantBufferRead)
                    .use_resource(arguments, ApplicationResourceUsage::StorageWrite)
                    .use_resource(self.statistics, ApplicationResourceUsage::StorageWrite),
                move |encoder| {
                    encoder.dispatch(
                        initialize_pipeline,
                        &initialize_shader,
                        &initialize_bindings,
                        [1, 1, 1],
                    )?;
                    Ok(())
                },
            )
            .map_err(|error| ApplicationDiagnosticError::msg(format!("{error:?}")))
            .context("adding GPU indirect-argument initialization to the render graph")?;

        let cull_pipeline = self.cull_pipeline;
        let cull_shader = self.cull_shader.clone();
        let cull_bindings = self.cull_bindings;
        let groups = [self.instance_count.div_ceil(THREADS_PER_GROUP), 1, 1];
        queue
            .enqueue(
                ApplicationRenderPassDesc::new("Nord GPU instance frustum culling")
                    .use_resource(self.constants, ApplicationResourceUsage::ConstantBufferRead)
                    .use_resource(self.source_srv_resource(), ApplicationResourceUsage::SampledRead)
                    .use_resource(self.visible_instances, ApplicationResourceUsage::StorageWrite)
                    .use_resource(arguments, ApplicationResourceUsage::StorageReadWrite)
                    .use_resource(self.statistics, ApplicationResourceUsage::StorageReadWrite),
                move |encoder| {
                    encoder.dispatch(cull_pipeline, &cull_shader, &cull_bindings, groups)?;
                    Ok(())
                },
            )
            .map_err(|error| ApplicationDiagnosticError::msg(format!("{error:?}")))
            .context("adding GPU frustum culling to the render graph")?;
        Ok(())
    }

    pub(super) fn enqueue_hiz_cull_pass(
        &self,
        queue: &mut ApplicationRenderQueue,
        hiz_resources: &[ApplicationGpuResourceHandle],
        hiz_views: &[ApplicationDescriptorHandle],
    ) -> nord_app::ApplicationResult<()> {
        if hiz_resources.len() != hiz_views.len() {
            return Err(ApplicationDiagnosticError::msg(
                "Hi-Z resource and descriptor counts do not match",
            ));
        }
        let pipeline = self.hiz_pipeline;
        let shader = self.hiz_shader.clone();
        let mut bindings = self
            .hiz_bindings
            .iter()
            .map(|binding| ApplicationPipelineDescriptorTableBinding {
                kind: binding.kind,
                register: binding.register,
                space: binding.space,
                descriptors: vec![binding.descriptor],
            })
            .collect::<Vec<_>>();
        bindings.push(ApplicationPipelineDescriptorTableBinding {
            kind: ApplicationShaderBindingKind::SampledResource,
            register: 1,
            space: 0,
            descriptors: hiz_views.to_vec(),
        });
        let groups = [self.instance_count.div_ceil(THREADS_PER_GROUP), 1, 1];
        let mut pass =
            ApplicationRenderPassDesc::new("Nord GPU frustum and Hi-Z occlusion culling")
                .use_resource(self.constants, ApplicationResourceUsage::ConstantBufferRead)
                .use_resource(self.source_instances, ApplicationResourceUsage::SampledRead)
                .use_resource(self.visible_instances, ApplicationResourceUsage::StorageWrite)
                .use_resource(self.arguments, ApplicationResourceUsage::StorageReadWrite)
                .use_resource(self.statistics, ApplicationResourceUsage::StorageReadWrite);
        for &resource in hiz_resources {
            pass = pass.use_resource(resource, ApplicationResourceUsage::SampledRead);
        }
        queue
            .enqueue(pass, move |encoder| {
                encoder.dispatch_with_descriptor_tables(pipeline, &shader, &bindings, groups)?;
                Ok(())
            })
            .map_err(|error| ApplicationDiagnosticError::msg(format!("{error:?}")))
            .context("adding GPU Hi-Z occlusion culling to the render graph")?;
        Ok(())
    }

    pub(super) fn enqueue_initialize_pass(
        &self,
        queue: &mut ApplicationRenderQueue,
    ) -> nord_app::ApplicationResult<()> {
        let pipeline = self.initialize_pipeline;
        let shader = self.initialize_shader.clone();
        let bindings = self.initialize_bindings;
        queue
            .enqueue(
                ApplicationRenderPassDesc::new("Nord GPU indirect argument initialization")
                    .use_resource(self.constants, ApplicationResourceUsage::ConstantBufferRead)
                    .use_resource(self.arguments, ApplicationResourceUsage::StorageWrite)
                    .use_resource(self.statistics, ApplicationResourceUsage::StorageWrite),
                move |encoder| {
                    encoder.dispatch(pipeline, &shader, &bindings, [1, 1, 1])?;
                    Ok(())
                },
            )
            .map_err(|error| ApplicationDiagnosticError::msg(format!("{error:?}")))
            .context("adding GPU indirect-argument initialization to render graph")?;
        Ok(())
    }

    const fn source_srv_resource(&self) -> ApplicationGpuResourceHandle {
        self.source_instances
    }

    pub(super) const fn visible_instances(&self) -> ApplicationGpuResourceHandle {
        self.visible_instances
    }

    pub(super) const fn visible_instances_size(&self) -> u32 {
        self.visible_instances_size
    }

    pub(super) const fn arguments(&self) -> ApplicationGpuResourceHandle {
        self.arguments
    }

    pub(super) const fn maximum_instances(&self) -> u32 {
        self.instance_count
    }

    pub(super) fn validate_gpu_output(
        &self,
        renderer: &mut ApplicationRenderer,
    ) -> nord_app::ApplicationResult<GpuCullingStats> {
        let readback = renderer
            .create_buffer(
                ApplicationBufferDesc::new(
                    VALIDATION_READBACK_BYTES,
                    ApplicationBufferUsage::COPY_DESTINATION,
                )?
                .with_memory(ApplicationBufferMemory::Readback),
            )
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating a one-shot indirect-argument validation readback")?;
        let read_result = (|| {
            renderer
                .copy_buffer_to_readback(self.arguments, 0, readback, 0, INDIRECT_ARGUMENT_BYTES)
                .map_err(ApplicationDiagnosticError::msg)
                .context("copying GPU-generated indirect arguments for validation")?;
            renderer
                .copy_buffer_to_readback(self.statistics, 0, readback, 20, 8)
                .map_err(ApplicationDiagnosticError::msg)
                .context("copying GPU culling statistics for validation")?;
            let bytes = renderer
                .read_buffer(readback, 0, INDIRECT_ARGUMENT_BYTE_COUNT + 8)
                .map_err(ApplicationDiagnosticError::msg)
                .context("reading GPU-generated indirect arguments")?;
            if bytes.len() != INDIRECT_ARGUMENT_BYTE_COUNT + 8 {
                return Err(ApplicationDiagnosticError::msg(
                    "GPU indirect-argument readback returned an unexpected byte count",
                ));
            }
            let values = bytes[..INDIRECT_ARGUMENT_BYTE_COUNT]
                .chunks_exact(4)
                .map(|word| u32::from_le_bytes(word.try_into().expect("four-byte word")))
                .collect::<Vec<_>>();
            let [index_count, instance_count, first_index, base_vertex, first_instance] =
                values.as_slice()
            else {
                return Err(ApplicationDiagnosticError::msg(
                    "GPU indirect-argument buffer does not contain five DWORDs",
                ));
            };
            if *index_count != self.index_count
                || *instance_count == 0
                || *instance_count > self.instance_count
                || *first_index != 0
                || *base_vertex != 0
                || *first_instance != 0
            {
                return Err(ApplicationDiagnosticError::msg(format!(
                    "GPU culling produced invalid indexed arguments: {values:?}"
                )));
            }
            let frustum_visible =
                u32::from_le_bytes(bytes[20..24].try_into().expect("four-byte counter"));
            let occlusion_culled =
                u32::from_le_bytes(bytes[24..28].try_into().expect("four-byte counter"));
            if frustum_visible < *instance_count
                || frustum_visible > self.instance_count
                || occlusion_culled > frustum_visible
                || frustum_visible - occlusion_culled != *instance_count
            {
                return Err(ApplicationDiagnosticError::msg(format!(
                    "GPU culling counters are inconsistent: final={instance_count}, frustum={frustum_visible}, occluded={occlusion_culled}"
                )));
            }
            Ok(GpuCullingStats { visible: *instance_count, frustum_visible, occlusion_culled })
        })();
        let destroy_result = renderer
            .destroy_resource(readback)
            .map_err(ApplicationDiagnosticError::msg)
            .context("releasing the one-shot indirect-argument validation readback");
        match (read_result, destroy_result) {
            (Ok(stats), Ok(())) => Ok(stats),
            (Err(error), _) | (Ok(_), Err(error)) => Err(error),
        }
    }

    pub(super) fn destroy(
        &self,
        renderer: &mut ApplicationRenderer,
    ) -> nord_app::ApplicationResult<()> {
        for descriptor in
            [self.visible_uav, self.arguments_uav, self.statistics_uav, self.source_srv]
        {
            renderer
                .destroy_descriptor(descriptor)
                .map_err(ApplicationDiagnosticError::msg)
                .context("retiring a GPU-culling descriptor")?;
        }
        for resource in [self.visible_instances, self.arguments, self.statistics, self.constants] {
            renderer
                .destroy_resource(resource)
                .map_err(ApplicationDiagnosticError::msg)
                .context("retiring a GPU-culling resource")?;
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
    .context("loading an embedded GPU-culling shader")
}

const fn binding(
    kind: ApplicationShaderBindingKind,
    register: u32,
    descriptor: ApplicationDescriptorHandle,
) -> ApplicationPipelineDescriptorBinding {
    ApplicationPipelineDescriptorBinding { kind, register, space: 0, descriptor }
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
