use nord_app::{
    ApplicationBufferDesc, ApplicationBufferMemory, ApplicationBufferUsage,
    ApplicationColorAttachmentBinding, ApplicationComputePipelineDesc, ApplicationCullMode,
    ApplicationDrawCall, ApplicationFrontFace, ApplicationGraphicsPipelineDesc,
    ApplicationIndexBufferBinding, ApplicationIndexFormat, ApplicationJobSystem,
    ApplicationPipelineDescriptorBinding, ApplicationPipelineDescriptorTableBinding,
    ApplicationPlatform, ApplicationPolygonFillMode, ApplicationPrimitiveTopology,
    ApplicationRenderBarrierKind, ApplicationRenderConfig, ApplicationRenderPassDesc,
    ApplicationRenderQueue, ApplicationRenderTargetDesc, ApplicationRenderer,
    ApplicationResourceUsage, ApplicationSamplerDesc, ApplicationScissorRect,
    ApplicationShaderBindingKind, ApplicationShaderCompileOptions, ApplicationShaderCompiler,
    ApplicationShaderResourceBinding, ApplicationShaderStage, ApplicationTextureDesc,
    ApplicationTextureFormat, ApplicationTextureUsage, ApplicationThreadPool,
    ApplicationThreadPoolConfig, ApplicationVertexAttribute, ApplicationVertexBufferBinding,
    ApplicationVertexFormat, ApplicationViewport, ApplicationWindowConfig,
};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn Error>> {
    let platform = ApplicationPlatform::initialize("nord-render-smoke")?;
    let window = platform.create_window(
        &ApplicationWindowConfig::new("Nord render resource validation", 128, 128)
            .with_visible(false),
    )?;

    let result = validate_resources(&platform, window);
    let destroy_result = platform.destroy_window(window);
    result?;
    destroy_result?;
    println!("RESULT: all runtime render-resource checks passed");
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_resources(
    platform: &ApplicationPlatform,
    window: nord_app::ApplicationWindowHandle,
) -> Result<(), Box<dyn Error>> {
    let pool = Arc::new(ApplicationThreadPool::new(ApplicationThreadPoolConfig::default())?);
    let jobs = ApplicationJobSystem::new(pool);
    let mut renderer =
        ApplicationRenderer::initialize(platform, window, ApplicationRenderConfig::new(128, 128))?;
    println!("PASS: D3D12 device, swapchain, and frame submission");

    let local_budget = renderer.memory_budget(nord_app::ApplicationGpuMemorySegment::Local)?;
    let non_local_budget =
        renderer.memory_budget(nord_app::ApplicationGpuMemorySegment::NonLocal)?;
    println!("PASS: DXGI local-memory budget query ({local_budget:?})");
    println!("PASS: DXGI non-local-memory budget query ({non_local_budget:?})");

    let device_buffer = renderer
        .create_buffer(ApplicationBufferDesc::new(
            256,
            ApplicationBufferUsage::COPY_DESTINATION
                .union(ApplicationBufferUsage::COPY_SOURCE)
                .union(ApplicationBufferUsage::VERTEX),
        )?)
        .map_err(|error| format!("creating device-local vertex buffer: {error}"))?;
    renderer
        .upload_buffer(device_buffer, 0, &[0x2A; 16])
        .map_err(|error| format!("uploading device-local vertex buffer: {error}"))?;
    println!("PASS: device-local buffer allocation and staged GPU upload");

    let vertex_buffer = renderer.create_buffer(ApplicationBufferDesc::new(
        9 * 5 * 4,
        ApplicationBufferUsage::COPY_DESTINATION.union(ApplicationBufferUsage::VERTEX),
    )?)?;
    let vertices = [
        [-0.90_f32, 0.70, 0.20, 0.25, 0.5],
        [0.65, 0.70, 0.20, 0.25, 0.5],
        [-0.10, -0.70, 0.20, 0.25, 0.5],
        [-0.75, 0.70, 0.80, 0.75, 0.5],
        [0.98, 0.70, 0.80, 0.75, 0.5],
        [0.20, -0.70, 0.80, 0.75, 0.5],
        [-0.25, 0.95, 0.10, 0.25, 0.5],
        [0.25, 0.95, 0.10, 0.25, 0.5],
        [0.00, 0.55, 0.10, 0.25, 0.5],
    ];
    let vertex_bytes =
        vertices.iter().flatten().flat_map(|value| value.to_le_bytes()).collect::<Vec<_>>();
    renderer.upload_buffer(vertex_buffer, 0, &vertex_bytes)?;
    let index_buffer = renderer.create_buffer(ApplicationBufferDesc::new(
        6 * 2,
        ApplicationBufferUsage::COPY_DESTINATION.union(ApplicationBufferUsage::INDEX),
    )?)?;
    let indices = [0_u16, 1, 2, 3, 4, 5];
    let index_bytes = indices.iter().flat_map(|value| value.to_le_bytes()).collect::<Vec<_>>();
    renderer.upload_buffer(index_buffer, 0, &index_bytes)?;
    println!("PASS: device-local vertex/index geometry upload");

    let constant_buffer = renderer.create_buffer(
        ApplicationBufferDesc::new(256, ApplicationBufferUsage::CONSTANT)?
            .with_memory(ApplicationBufferMemory::Upload),
    )?;
    let white_tint = [1.0_f32; 4].map(f32::to_le_bytes).concat();
    renderer.write_buffer(constant_buffer, 0, &white_tint)?;
    let constant_view = renderer.create_constant_buffer_view(constant_buffer)?;
    assert_eq!(constant_view.kind(), nord_app::ApplicationDescriptorKind::Resource);
    println!("PASS: upload buffer write and CBV creation");
    let constant_view_array =
        renderer.create_constant_buffer_view_array(&[constant_buffer, constant_buffer])?;
    assert_contiguous(&constant_view_array);
    println!("PASS: contiguous CBV descriptor-array creation");

    let readback_buffer = renderer.create_buffer(
        ApplicationBufferDesc::new(16, ApplicationBufferUsage::COPY_DESTINATION)?
            .with_memory(ApplicationBufferMemory::Readback),
    )?;
    renderer.copy_buffer_to_readback(device_buffer, 0, readback_buffer, 0, 16)?;
    let readback = renderer.read_buffer(readback_buffer, 0, 16)?;
    assert_eq!(readback.len(), 16, "readback mapping returned the wrong range length");
    assert_eq!(readback, vec![0x2A; 16], "GPU buffer upload/readback round-trip changed data");
    println!("PASS: buffer copy, readback mapping, and byte-for-byte upload round-trip");

    let sampled_texture = renderer.create_texture(
        ApplicationTextureDesc::new(
            4,
            4,
            ApplicationTextureFormat::Rgba8Unorm,
            ApplicationTextureUsage::SAMPLED.union(ApplicationTextureUsage::COPY_DESTINATION),
        )?
        .with_mip_levels(3)?,
    )?;
    let mut checker_bytes = Vec::with_capacity(64);
    for _row in 0..4 {
        for column in 0..4 {
            checker_bytes.extend_from_slice(if column < 2 {
                &[255, 0, 0, 255]
            } else {
                &[0, 0, 255, 255]
            });
        }
    }
    renderer.upload_texture(sampled_texture, 0, 0, 16, &checker_bytes)?;
    renderer.upload_texture(sampled_texture, 1, 0, 8, &[0x55; 16])?;
    renderer.upload_texture(sampled_texture, 2, 0, 4, &[0x33; 4])?;
    let texture_view = renderer.create_texture_srv(sampled_texture)?;
    assert_eq!(texture_view.kind(), nord_app::ApplicationDescriptorKind::Resource);
    println!("PASS: texture creation, subresource upload, and SRV creation");

    let array_texture = renderer.create_texture(
        ApplicationTextureDesc::new(
            2,
            2,
            ApplicationTextureFormat::Rgba8Unorm,
            ApplicationTextureUsage::SAMPLED.union(ApplicationTextureUsage::COPY_DESTINATION),
        )?
        .with_array_layers(2)?,
    )?;
    renderer.upload_texture(array_texture, 0, 0, 8, &[0x21; 16])?;
    renderer.upload_texture(array_texture, 0, 1, 8, &[0x42; 16])?;
    let array_view = renderer.create_texture_srv(array_texture)?;
    assert_eq!(array_view.kind(), nord_app::ApplicationDescriptorKind::Resource);
    println!("PASS: array texture layer uploads and array SRV creation");
    let sampled_texture_table =
        renderer.create_texture_srv_array(&[sampled_texture, array_texture])?;
    assert_eq!(sampled_texture_table.len(), 2);
    assert_eq!(sampled_texture_table[1].index(), sampled_texture_table[0].index() + 1);
    println!("PASS: contiguous SRV descriptor-table allocation for a texture array");

    let storage_buffer = renderer
        .create_buffer(ApplicationBufferDesc::new(256, ApplicationBufferUsage::STORAGE)?)?;
    let storage_buffer_view = renderer.create_unordered_access_view(storage_buffer)?;
    assert_eq!(storage_buffer_view.kind(), nord_app::ApplicationDescriptorKind::Resource);
    let storage_texture = renderer.create_texture(ApplicationTextureDesc::new(
        4,
        4,
        ApplicationTextureFormat::Rgba8Unorm,
        ApplicationTextureUsage::STORAGE,
    )?)?;
    let storage_texture_array_member = renderer.create_texture(ApplicationTextureDesc::new(
        4,
        4,
        ApplicationTextureFormat::Rgba8Unorm,
        ApplicationTextureUsage::STORAGE,
    )?)?;
    let storage_texture_view = renderer.create_unordered_access_view(storage_texture)?;
    assert_eq!(storage_texture_view.kind(), nord_app::ApplicationDescriptorKind::Resource);
    println!("PASS: storage buffer/texture allocations and UAV creation");
    let storage_buffer_views =
        renderer.create_unordered_access_view_array(&[storage_buffer, storage_buffer])?;
    let storage_texture_views = renderer
        .create_unordered_access_view_array(&[storage_texture, storage_texture_array_member])?;
    assert_contiguous(&storage_buffer_views);
    assert_contiguous(&storage_texture_views);
    println!("PASS: contiguous raw-buffer and texture UAV descriptor arrays");

    let target = renderer.create_render_target(ApplicationRenderTargetDesc::new(
        32,
        32,
        Some(ApplicationTextureFormat::Rgba8Unorm),
        Some(ApplicationTextureFormat::D32Float),
    )?)?;
    let color_target = target.color.expect("requested color attachment");
    let depth_target = target.depth.expect("requested depth attachment");
    let render_target_view = renderer.create_render_target_view(color_target)?;
    let depth_stencil_view = renderer.create_depth_stencil_view(depth_target)?;
    let auxiliary_target = renderer
        .create_render_target(ApplicationRenderTargetDesc::new(
            32,
            32,
            Some(ApplicationTextureFormat::Rgba8Unorm),
            None,
        )?)?
        .color
        .expect("requested auxiliary color attachment");
    let auxiliary_target_view = renderer.create_render_target_view(auxiliary_target)?;
    assert_eq!(render_target_view.kind(), nord_app::ApplicationDescriptorKind::RenderTarget);
    assert_eq!(depth_stencil_view.kind(), nord_app::ApplicationDescriptorKind::DepthStencil);
    println!("PASS: color/depth render-target allocation and RTV/DSV creation");

    let sampler = renderer.create_sampler(ApplicationSamplerDesc::linear_repeat())?;
    let sampler_array = renderer.create_sampler_array(&[
        ApplicationSamplerDesc::linear_repeat(),
        ApplicationSamplerDesc::linear_repeat(),
    ])?;
    assert_contiguous(&sampler_array);
    println!("PASS: sampler descriptor creation");

    let shader_path = shader_fixture_path()?;
    let compiler = ApplicationShaderCompiler::default();
    let vertex = compiler.compile(&ApplicationShaderCompileOptions::new(
        &shader_path,
        ApplicationShaderStage::Vertex,
        "VSMain",
    ))?;
    let pixel = compiler.compile(&ApplicationShaderCompileOptions::new(
        &shader_path,
        ApplicationShaderStage::Pixel,
        "PSMain",
    ))?;
    let compute = compiler.compile(&ApplicationShaderCompileOptions::new(
        &shader_path,
        ApplicationShaderStage::Compute,
        "CSMain",
    ))?;
    let compute_array = compiler.compile(&ApplicationShaderCompileOptions::new(
        &shader_path,
        ApplicationShaderStage::Compute,
        "CSArrayMain",
    ))?;
    assert!(has_binding(
        pixel.reflection.bindings(),
        "frameData",
        ApplicationShaderBindingKind::ConstantBuffer,
        0,
    ));
    assert_eq!(
        pixel
            .reflection
            .bindings()
            .iter()
            .find(|binding| binding.name == "frameData")
            .expect("DXC should reflect the constant-buffer array")
            .count,
        2
    );
    let reflected_texture_array = pixel
        .reflection
        .bindings()
        .iter()
        .find(|binding| binding.name == "sourceTextures")
        .expect("DXC should reflect the texture array");
    assert_eq!(reflected_texture_array.count, 2);
    assert!(has_binding(
        pixel.reflection.bindings(),
        "sourceTextures",
        ApplicationShaderBindingKind::SampledResource,
        0,
    ));
    assert_eq!(
        compute_array
            .reflection
            .bindings()
            .iter()
            .find(|binding| binding.name == "outputTextures")
            .expect("DXC should reflect the UAV array")
            .count,
        2
    );
    assert!(has_binding(
        pixel.reflection.bindings(),
        "sourceSamplers",
        ApplicationShaderBindingKind::Sampler,
        0,
    ));
    assert_eq!(
        pixel
            .reflection
            .bindings()
            .iter()
            .find(|binding| binding.name == "sourceSamplers")
            .expect("DXC should reflect the sampler array")
            .count,
        2
    );
    assert!(has_binding(
        compute.reflection.bindings(),
        "ComputeData",
        ApplicationShaderBindingKind::ConstantBuffer,
        1,
    ));
    assert!(has_binding(
        compute.reflection.bindings(),
        "outputTexture",
        ApplicationShaderBindingKind::StorageResource,
        0,
    ));
    println!("PASS: DXC compile, native reflection, and binding classification for VS/PS/CS");

    let mut graphics_desc = ApplicationGraphicsPipelineDesc::new(
        vertex.shader_id,
        pixel.shader_id,
        ApplicationTextureFormat::Rgba8Unorm,
    );
    graphics_desc.depth_format = Some(ApplicationTextureFormat::D32Float);
    graphics_desc.depth_test = true;
    graphics_desc.depth_write = true;
    graphics_desc.topology = ApplicationPrimitiveTopology::Triangle;
    graphics_desc.cull_mode = ApplicationCullMode::None;
    graphics_desc.front_face = ApplicationFrontFace::CounterClockwise;
    graphics_desc.fill_mode = ApplicationPolygonFillMode::Solid;
    graphics_desc = graphics_desc
        .with_color_attachment_format(ApplicationTextureFormat::Rgba8Unorm)
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
            semantic: "TEXCOORD".into(),
            semantic_index: 0,
            format: ApplicationVertexFormat::Float2,
            input_slot: 0,
            byte_offset: 12,
            per_instance: false,
            instance_step_rate: 0,
        });
    let graphics_pipeline =
        renderer.create_graphics_pipeline(&graphics_desc, &vertex, &pixel, 1)?;
    let graphics_pipeline_reused =
        renderer.create_graphics_pipeline(&graphics_desc, &vertex, &pixel, 2)?;
    assert_eq!(graphics_pipeline.native_id, graphics_pipeline_reused.native_id);
    assert_eq!(graphics_pipeline_reused.last_used_frame, 2);
    let compute_pipeline = renderer.create_compute_pipeline(
        ApplicationComputePipelineDesc { compute_shader: compute.shader_id },
        &compute,
        2,
    )?;
    assert_eq!(compute_pipeline.last_used_frame, 2);
    let compute_array_pipeline = renderer.create_compute_pipeline(
        ApplicationComputePipelineDesc { compute_shader: compute_array.shader_id },
        &compute_array,
        2,
    )?;
    assert_eq!(compute_array_pipeline.last_used_frame, 2);
    println!("PASS: native graphics/compute PSOs, root signatures, and cache reuse");

    let graphics_bindings = [
        ApplicationPipelineDescriptorTableBinding {
            kind: ApplicationShaderBindingKind::ConstantBuffer,
            register: 0,
            space: 0,
            descriptors: constant_view_array.clone(),
        },
        ApplicationPipelineDescriptorTableBinding {
            kind: ApplicationShaderBindingKind::SampledResource,
            register: 0,
            space: 0,
            descriptors: sampled_texture_table,
        },
        ApplicationPipelineDescriptorTableBinding {
            kind: ApplicationShaderBindingKind::Sampler,
            register: 0,
            space: 0,
            descriptors: sampler_array.clone(),
        },
    ];
    let compute_bindings = [
        ApplicationPipelineDescriptorBinding {
            kind: ApplicationShaderBindingKind::ConstantBuffer,
            register: 1,
            space: 0,
            descriptor: constant_view,
        },
        ApplicationPipelineDescriptorBinding {
            kind: ApplicationShaderBindingKind::StorageResource,
            register: 0,
            space: 0,
            descriptor: storage_texture_view,
        },
    ];
    let array_compute_bindings = [
        ApplicationPipelineDescriptorTableBinding {
            kind: ApplicationShaderBindingKind::ConstantBuffer,
            register: 1,
            space: 0,
            descriptors: vec![constant_view],
        },
        ApplicationPipelineDescriptorTableBinding {
            kind: ApplicationShaderBindingKind::StorageResource,
            register: 1,
            space: 0,
            descriptors: storage_texture_views.clone(),
        },
    ];
    let mut invalid_graph = ApplicationRenderQueue::new();
    invalid_graph.enqueue(
        ApplicationRenderPassDesc::new("reject undeclared shader resource")
            .use_resource(constant_buffer, ApplicationResourceUsage::ConstantBufferRead),
        {
            let compute = compute.clone();
            move |encoder| {
                encoder.dispatch(
                    ApplicationComputePipelineDesc { compute_shader: compute.shader_id },
                    &compute,
                    &compute_bindings,
                    [1, 1, 1],
                )?;
                Ok(())
            }
        },
    )?;
    assert!(
        renderer.submit_render_queue(&mut invalid_graph, &jobs).is_err(),
        "a pass must declare every GPU resource referenced by shader descriptors"
    );
    println!("PASS: graph rejects undeclared shader descriptor resources before submission");

    let mut queue = ApplicationRenderQueue::new();
    // Keep a genuinely barrier-free pass at slot zero. The barrier recorder
    // must preserve graph-pass indices while worker partitions finish out of order.
    queue
        .enqueue(ApplicationRenderPassDesc::new("empty synchronization slot"), |_encoder| Ok(()))?;
    queue.enqueue(
        ApplicationRenderPassDesc::new("two indexed meshes")
            .use_resource(constant_buffer, ApplicationResourceUsage::ConstantBufferRead)
            .use_resource(sampled_texture, ApplicationResourceUsage::SampledRead)
            .use_resource(array_texture, ApplicationResourceUsage::SampledRead)
            .use_resource(color_target, ApplicationResourceUsage::ColorAttachmentWrite)
            .use_resource(auxiliary_target, ApplicationResourceUsage::ColorAttachmentWrite)
            .use_resource(depth_target, ApplicationResourceUsage::DepthAttachmentWrite)
            .use_resource(vertex_buffer, ApplicationResourceUsage::VertexBufferRead)
            .use_resource(index_buffer, ApplicationResourceUsage::IndexBufferRead),
        {
            let graphics_desc = graphics_desc.clone();
            let vertex = vertex.clone();
            let pixel = pixel.clone();
            move |encoder| {
                encoder.begin_graphics_pass_with_attachments(
                    &[
                        ApplicationColorAttachmentBinding {
                            resource: color_target,
                            view: render_target_view,
                            clear_color: nord_app::ApplicationClearColor::rgba(
                                0.01, 0.02, 0.03, 1.0,
                            ),
                        },
                        ApplicationColorAttachmentBinding {
                            resource: auxiliary_target,
                            view: auxiliary_target_view,
                            clear_color: nord_app::ApplicationClearColor::rgba(0.0, 0.0, 0.0, 1.0),
                        },
                    ],
                    Some((depth_target, depth_stencil_view)),
                    1.0,
                    Some(ApplicationViewport::new(0.0, 0.0, 32.0, 32.0)),
                    Some(ApplicationScissorRect::new(8, 4, 32, 28)),
                )?;
                encoder.draw_with_descriptor_tables(
                    &graphics_desc,
                    &vertex,
                    &pixel,
                    &graphics_bindings,
                    &[ApplicationVertexBufferBinding {
                        slot: 0,
                        buffer: vertex_buffer,
                        offset: 0,
                        size: u32::try_from(vertex_bytes.len()).expect("vertex test data fits u32"),
                        stride: 20,
                    }],
                    Some(ApplicationIndexBufferBinding {
                        buffer: index_buffer,
                        offset: 0,
                        size: u32::try_from(index_bytes.len()).expect("index test data fits u32"),
                        format: ApplicationIndexFormat::Uint16,
                    }),
                    ApplicationDrawCall::Indexed {
                        index_count: 3,
                        instance_count: 1,
                        first_index: 0,
                        base_vertex: 0,
                        first_instance: 0,
                    },
                )?;
                encoder.draw_with_descriptor_tables(
                    &graphics_desc,
                    &vertex,
                    &pixel,
                    &graphics_bindings,
                    &[ApplicationVertexBufferBinding {
                        slot: 0,
                        buffer: vertex_buffer,
                        offset: 0,
                        size: u32::try_from(vertex_bytes.len()).expect("vertex test data fits u32"),
                        stride: 20,
                    }],
                    Some(ApplicationIndexBufferBinding {
                        buffer: index_buffer,
                        offset: 0,
                        size: u32::try_from(index_bytes.len()).expect("index test data fits u32"),
                        format: ApplicationIndexFormat::Uint16,
                    }),
                    ApplicationDrawCall::Indexed {
                        index_count: 3,
                        instance_count: 1,
                        first_index: 3,
                        base_vertex: 0,
                        first_instance: 0,
                    },
                )?;
                encoder.draw_with_descriptor_tables(
                    &graphics_desc,
                    &vertex,
                    &pixel,
                    &graphics_bindings,
                    &[ApplicationVertexBufferBinding {
                        slot: 0,
                        buffer: vertex_buffer,
                        offset: 0,
                        size: u32::try_from(vertex_bytes.len()).expect("vertex test data fits u32"),
                        stride: 20,
                    }],
                    None,
                    ApplicationDrawCall::Vertices {
                        vertex_count: 3,
                        instance_count: 1,
                        first_vertex: 6,
                        first_instance: 0,
                    },
                )?;
                encoder.end_graphics_pass()?;
                Ok(())
            }
        },
    )?;
    queue.enqueue(
        ApplicationRenderPassDesc::new("compute storage output")
            .use_resource(constant_buffer, ApplicationResourceUsage::ConstantBufferRead)
            .use_resource(storage_texture, ApplicationResourceUsage::StorageWrite),
        {
            let compute = compute.clone();
            move |encoder| {
                encoder.dispatch(
                    ApplicationComputePipelineDesc { compute_shader: compute.shader_id },
                    &compute,
                    &compute_bindings,
                    [4, 4, 1],
                )?;
                Ok(())
            }
        },
    )?;
    queue.enqueue(
        ApplicationRenderPassDesc::new("ordered compute write")
            .use_resource(constant_buffer, ApplicationResourceUsage::ConstantBufferRead)
            .use_resource(storage_texture, ApplicationResourceUsage::StorageWrite),
        {
            let compute = compute.clone();
            move |encoder| {
                encoder.dispatch(
                    ApplicationComputePipelineDesc { compute_shader: compute.shader_id },
                    &compute,
                    &compute_bindings,
                    [4, 4, 1],
                )?;
                Ok(())
            }
        },
    )?;
    queue.enqueue(
        ApplicationRenderPassDesc::new("compute descriptor-array outputs")
            .use_resource(constant_buffer, ApplicationResourceUsage::ConstantBufferRead)
            .use_resource(storage_texture, ApplicationResourceUsage::StorageWrite)
            .use_resource(storage_texture_array_member, ApplicationResourceUsage::StorageWrite),
        {
            let compute_array = compute_array.clone();
            move |encoder| {
                encoder.dispatch_with_descriptor_tables(
                    ApplicationComputePipelineDesc { compute_shader: compute_array.shader_id },
                    &compute_array,
                    &array_compute_bindings,
                    [4, 4, 1],
                )?;
                Ok(())
            }
        },
    )?;
    queue.enqueue(
        ApplicationRenderPassDesc::new("GPU result readback")
            .use_resource(color_target, ApplicationResourceUsage::CopySource)
            .use_resource(storage_texture, ApplicationResourceUsage::CopySource)
            .use_resource(storage_texture_array_member, ApplicationResourceUsage::CopySource),
        move |_encoder| Ok(()),
    )?;
    let transient_desc = ApplicationTextureDesc::new(
        32,
        32,
        ApplicationTextureFormat::Rgba8Unorm,
        ApplicationTextureUsage::STORAGE,
    )?;
    let transient_first = queue.create_transient_texture(transient_desc)?;
    let transient_second = queue.create_transient_texture(transient_desc)?;
    queue.enqueue(
        ApplicationRenderPassDesc::new("transient compute write one")
            .use_resource(constant_buffer, ApplicationResourceUsage::ConstantBufferRead)
            .use_resource(transient_first, ApplicationResourceUsage::StorageWrite),
        {
            let compute = compute.clone();
            move |encoder| {
                let bindings = [ApplicationPipelineDescriptorBinding {
                    kind: ApplicationShaderBindingKind::ConstantBuffer,
                    register: 1,
                    space: 0,
                    descriptor: constant_view,
                }];
                encoder.dispatch_transient_uav(
                    ApplicationComputePipelineDesc { compute_shader: compute.shader_id },
                    &compute,
                    &bindings,
                    transient_first,
                    0,
                    0,
                    [4, 4, 1],
                )?;
                Ok(())
            }
        },
    )?;
    queue.enqueue(
        ApplicationRenderPassDesc::new("transient compute write two")
            .use_resource(constant_buffer, ApplicationResourceUsage::ConstantBufferRead)
            .use_resource(transient_second, ApplicationResourceUsage::StorageWrite),
        {
            let compute = compute.clone();
            move |encoder| {
                let bindings = [ApplicationPipelineDescriptorBinding {
                    kind: ApplicationShaderBindingKind::ConstantBuffer,
                    register: 1,
                    space: 0,
                    descriptor: constant_view,
                }];
                encoder.dispatch_transient_uav(
                    ApplicationComputePipelineDesc { compute_shader: compute.shader_id },
                    &compute,
                    &bindings,
                    transient_second,
                    0,
                    0,
                    [4, 4, 1],
                )?;
                Ok(())
            }
        },
    )?;
    let graph = renderer
        .submit_render_queue(&mut queue, &jobs)
        .map_err(|error| format!("submitting render queue: {error:?}"))?;
    let left_pixel = renderer.read_texture_pixel(color_target, 12, 16)?;
    let right_pixel = renderer.read_texture_pixel(color_target, 24, 16)?;
    let clear_pixel = renderer.read_texture_pixel(color_target, 16, 16)?;
    let non_indexed_pixel = renderer.read_texture_pixel(color_target, 16, 6)?;
    let auxiliary_pixel = renderer.read_texture_pixel(auxiliary_target, 12, 16)?;
    let scissor_clipped_pixel = renderer.read_texture_pixel(color_target, 4, 16)?;
    assert_eq!(
        left_pixel,
        [255, 0, 0, 255],
        "first indexed mesh was cleared or sampled incorrectly"
    );
    assert_eq!(right_pixel, [0, 0, 255, 255], "second indexed mesh did not reach the target");
    assert_eq!(clear_pixel, [255, 0, 0, 255], "near mesh must win the overlapping depth test");
    assert_eq!(non_indexed_pixel, [255, 0, 0, 255], "non-indexed draw did not reach the target");
    assert_eq!(auxiliary_pixel, [0, 255, 0, 255], "second pixel-shader output missed its target");
    assert_eq!(
        scissor_clipped_pixel,
        [2, 5, 8, 255],
        "scissor rectangle did not clip outside pixels"
    );
    println!(
        "PASS: indexed/non-indexed meshes, depth, and two color outputs; left={left_pixel:?}, right={right_pixel:?}, overlap={clear_pixel:?}, non-indexed={non_indexed_pixel:?}, auxiliary={auxiliary_pixel:?}"
    );
    let compute_pixel = renderer.read_texture_pixel(storage_texture, 2, 2)?;
    assert_eq!(compute_pixel, [255; 4], "compute shader result did not reach the UAV texture");
    let compute_array_pixel = renderer.read_texture_pixel(storage_texture_array_member, 2, 2)?;
    assert_eq!(
        compute_array_pixel, [255; 4],
        "compute UAV descriptor array did not write its second resource"
    );
    println!(
        "PASS: compute scalar/UAV-array passes read back {compute_pixel:?} and {compute_array_pixel:?}"
    );
    assert_eq!(graph.passes().len(), 8);
    assert!(graph.dependency_count() >= 4);
    assert!(graph.barrier_count() >= 10);
    assert!(
        graph.passes()[3]
            .barriers_before
            .iter()
            .any(|barrier| barrier.kind == ApplicationRenderBarrierKind::UavOrdering)
    );
    assert!(
        graph.passes()[7]
            .barriers_before
            .iter()
            .any(|barrier| { barrier.kind == ApplicationRenderBarrierKind::Aliasing })
    );
    println!(
        "PASS: render queue compiled and executed {} dependency-ordered passes ({} edges, {} barriers)",
        graph.passes().len(),
        graph.dependency_count(),
        graph.barrier_count()
    );
    queue.clear();

    let stats = renderer.gpu_allocator_stats();
    assert!(stats.allocation_count >= 8, "expected resource allocations to be tracked: {stats:?}");
    assert!(stats.live_bytes > 0, "live GPU allocations were not accounted: {stats:?}");
    let active_budget = renderer.memory_budget(nord_app::ApplicationGpuMemorySegment::Local)?;
    renderer.render_frame(nord_app::ApplicationClearColor::rgba(0.1, 0.2, 0.3, 1.0))?;
    renderer.wait_idle()?;
    println!("PASS: allocator accounting and frame execution ({stats:?}, {active_budget:?})");

    renderer.destroy_descriptor(sampler)?;
    for descriptor in sampler_array {
        renderer.destroy_descriptor(descriptor)?;
    }
    for resource in [
        device_buffer,
        vertex_buffer,
        index_buffer,
        constant_buffer,
        readback_buffer,
        sampled_texture,
        array_texture,
        auxiliary_target,
        storage_buffer,
        storage_texture,
        storage_texture_array_member,
        color_target,
        depth_target,
    ] {
        renderer.destroy_resource(resource)?;
    }
    assert!(renderer.create_texture_srv(sampled_texture).is_err());
    let final_stats = renderer.gpu_allocator_stats();
    assert_eq!(final_stats.live_bytes, 0, "GPU resource destruction leaked live allocations");
    assert_eq!(final_stats.allocation_count, 0);
    assert!(renderer.destroy_descriptor(texture_view).is_err());

    let oversized_block_bytes = 65 * 1024 * 1024;
    let oversized_buffer = ApplicationBufferDesc::new(
        oversized_block_bytes,
        ApplicationBufferUsage::COPY_DESTINATION,
    )?;
    let first_large = renderer.create_buffer(oversized_buffer)?;
    let second_large = renderer.create_buffer(oversized_buffer)?;
    renderer.destroy_resource(first_large)?;
    renderer.destroy_resource(second_large)?;
    let released = renderer.trim_memory_pools()?;
    assert!(
        released >= 64 * 1024 * 1024,
        "trim did not release the excess empty heaps: {released}"
    );
    let trimmed_budget = renderer.memory_budget(nord_app::ApplicationGpuMemorySegment::Local)?;
    assert!(renderer.trim_pipeline_cache(u64::MAX)? >= 2);
    println!(
        "PASS: descriptor/resource retirement, stale-handle rejection, and pool/cache trim ({released} bytes, {trimmed_budget:?})"
    );

    renderer.wait_idle()?;
    Ok(())
}

fn has_binding(
    bindings: &[ApplicationShaderResourceBinding],
    expected_name: &str,
    expected_kind: ApplicationShaderBindingKind,
    expected_register: u32,
) -> bool {
    bindings.iter().any(|binding| {
        binding.name == expected_name
            && binding.kind == expected_kind
            && binding.register == expected_register
            && binding.space == 0
    })
}

fn assert_contiguous(descriptors: &[nord_app::ApplicationDescriptorHandle]) {
    assert!(!descriptors.is_empty());
    assert!(descriptors.windows(2).all(|pair| pair[1].index() == pair[0].index() + 1));
}

fn shader_fixture_path() -> Result<PathBuf, Box<dyn Error>> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("render-smoke");
    fs::create_dir_all(&directory)?;
    let source = directory.join("render_resource_smoke.hlsl");
    fs::write(&source, include_str!("render_resource_smoke.hlsl"))?;
    Ok(source)
}
