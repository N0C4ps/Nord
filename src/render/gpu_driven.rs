//! Game-owned GPU culling policy and its renderer resource bindings.

use nord_app::{
    ApplicationBufferDesc, ApplicationBufferMemory, ApplicationBufferUsage,
    ApplicationCompiledShader, ApplicationComputePipelineDesc, ApplicationDescriptorHandle,
    ApplicationDiagnosticError, ApplicationErrorContext, ApplicationGpuResourceHandle,
    ApplicationPipelineDescriptorBinding, ApplicationRenderPassDesc, ApplicationRenderQueue,
    ApplicationRenderer, ApplicationResourceUsage, ApplicationShaderBindingKind,
    ApplicationShaderStage,
};

const CONSTANT_BYTES: usize = 256;
const INDIRECT_ARGUMENT_BYTES: u64 = 20;
const INDIRECT_ARGUMENT_BYTE_COUNT: usize = 20;
const THREADS_PER_GROUP: u32 = 64;

pub(super) struct GpuDrivenCulling {
    source_instances: ApplicationGpuResourceHandle,
    visible_instances: ApplicationGpuResourceHandle,
    visible_instances_size: u32,
    visible_uav: ApplicationDescriptorHandle,
    arguments: ApplicationGpuResourceHandle,
    arguments_uav: ApplicationDescriptorHandle,
    source_srv: ApplicationDescriptorHandle,
    constants: ApplicationGpuResourceHandle,
    initialize_pipeline: ApplicationComputePipelineDesc,
    initialize_shader: ApplicationCompiledShader,
    initialize_bindings: [ApplicationPipelineDescriptorBinding; 2],
    cull_pipeline: ApplicationComputePipelineDesc,
    cull_shader: ApplicationCompiledShader,
    cull_bindings: [ApplicationPipelineDescriptorBinding; 4],
    instance_count: u32,
    index_count: u32,
    bounds_radius: f32,
}

impl GpuDrivenCulling {
    #[allow(clippy::too_many_lines)]
    pub(super) fn create(
        renderer: &mut ApplicationRenderer,
        source_instances: ApplicationGpuResourceHandle,
        instance_bytes: u32,
        instance_count: u32,
        index_count: u32,
        bounds_radius: f32,
    ) -> nord_app::ApplicationResult<Self> {
        if instance_bytes == 0
            || !instance_bytes.is_multiple_of(24)
            || instance_count == 0
            || index_count == 0
            || !bounds_radius.is_finite()
            || bounds_radius <= 0.0
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
        let initialize_pipeline =
            ApplicationComputePipelineDesc { compute_shader: initialize_shader.shader_id };
        let cull_pipeline =
            ApplicationComputePipelineDesc { compute_shader: cull_shader.shader_id };
        renderer
            .create_compute_pipeline(initialize_pipeline, &initialize_shader, 0)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the indirect-argument initialization compute pipeline")?;
        renderer
            .create_compute_pipeline(cull_pipeline, &cull_shader, 0)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the GPU instance-culling compute pipeline")?;
        let initialize_bindings = [
            binding(ApplicationShaderBindingKind::ConstantBuffer, 0, constant_view),
            binding(ApplicationShaderBindingKind::StorageResource, 0, arguments_uav),
        ];
        let cull_bindings = [
            binding(ApplicationShaderBindingKind::ConstantBuffer, 0, constant_view),
            binding(ApplicationShaderBindingKind::SampledResource, 0, source_srv),
            binding(ApplicationShaderBindingKind::StorageResource, 0, visible_uav),
            binding(ApplicationShaderBindingKind::StorageResource, 1, arguments_uav),
        ];
        Ok(Self {
            source_instances,
            visible_instances,
            visible_instances_size: instance_bytes,
            visible_uav,
            arguments,
            arguments_uav,
            source_srv,
            constants,
            initialize_pipeline,
            initialize_shader,
            initialize_bindings,
            cull_pipeline,
            cull_shader,
            cull_bindings,
            instance_count,
            index_count,
            bounds_radius,
        })
    }

    pub(super) fn update_constants(
        &self,
        renderer: &mut ApplicationRenderer,
        planes: [nord_app::ApplicationPlane3; 6],
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
                    .use_resource(arguments, ApplicationResourceUsage::StorageWrite),
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
                    .use_resource(arguments, ApplicationResourceUsage::StorageReadWrite),
                move |encoder| {
                    encoder.dispatch(cull_pipeline, &cull_shader, &cull_bindings, groups)?;
                    Ok(())
                },
            )
            .map_err(|error| ApplicationDiagnosticError::msg(format!("{error:?}")))
            .context("adding GPU frustum culling to the render graph")?;
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
    ) -> nord_app::ApplicationResult<u32> {
        let readback = renderer
            .create_buffer(
                ApplicationBufferDesc::new(
                    INDIRECT_ARGUMENT_BYTES,
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
            let bytes = renderer
                .read_buffer(readback, 0, INDIRECT_ARGUMENT_BYTE_COUNT)
                .map_err(ApplicationDiagnosticError::msg)
                .context("reading GPU-generated indirect arguments")?;
            if bytes.len() != INDIRECT_ARGUMENT_BYTE_COUNT {
                return Err(ApplicationDiagnosticError::msg(
                    "GPU indirect-argument readback returned an unexpected byte count",
                ));
            }
            let values = bytes
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
            Ok(*instance_count)
        })();
        let destroy_result = renderer
            .destroy_resource(readback)
            .map_err(ApplicationDiagnosticError::msg)
            .context("releasing the one-shot indirect-argument validation readback");
        match (read_result, destroy_result) {
            (Ok(count), Ok(())) => Ok(count),
            (Err(error), _) | (Ok(_), Err(error)) => Err(error),
        }
    }

    pub(super) fn destroy(
        &self,
        renderer: &mut ApplicationRenderer,
    ) -> nord_app::ApplicationResult<()> {
        for descriptor in [self.visible_uav, self.arguments_uav, self.source_srv] {
            renderer
                .destroy_descriptor(descriptor)
                .map_err(ApplicationDiagnosticError::msg)
                .context("retiring a GPU-culling descriptor")?;
        }
        for resource in [self.visible_instances, self.arguments, self.constants] {
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
