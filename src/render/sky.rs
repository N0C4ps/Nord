//! Procedural game sky gradient.

use nord_app::{
    ApplicationBufferDesc, ApplicationBufferMemory, ApplicationBufferUsage,
    ApplicationCompiledShader, ApplicationCullMode, ApplicationDiagnosticError,
    ApplicationErrorContext, ApplicationGraphicsPipelineDesc, ApplicationRenderer,
    ApplicationShaderStage, ApplicationTextureFormat, ApplicationVertexAttribute,
    ApplicationVertexFormat,
};

const VERTEX_BYTES: u64 = 120;

pub(super) struct Sky {
    pipeline: ApplicationGraphicsPipelineDesc,
    vertex_shader: ApplicationCompiledShader,
    pixel_shader: ApplicationCompiledShader,
    vertex_buffer: nord_app::ApplicationGpuResourceHandle,
}

impl Sky {
    pub(super) fn create(renderer: &mut ApplicationRenderer) -> nord_app::ApplicationResult<Self> {
        let vertex_shader = ApplicationCompiledShader::from_precompiled(
            ApplicationShaderStage::Vertex,
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_sky.vs.dxil")).to_vec(),
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_sky.vs.reflection")).to_vec(),
            include_str!(concat!(env!("OUT_DIR"), "/nord_sky.vs.asm")).to_owned(),
        )
        .map_err(ApplicationDiagnosticError::msg)
        .context("loading the game's sky vertex shader")?;
        let pixel_shader = ApplicationCompiledShader::from_precompiled(
            ApplicationShaderStage::Pixel,
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_sky.ps.dxil")).to_vec(),
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_sky.ps.reflection")).to_vec(),
            include_str!(concat!(env!("OUT_DIR"), "/nord_sky.ps.asm")).to_owned(),
        )
        .map_err(ApplicationDiagnosticError::msg)
        .context("loading the game's sky pixel shader")?;

        let mut pipeline = ApplicationGraphicsPipelineDesc::new(
            vertex_shader.shader_id,
            pixel_shader.shader_id,
            ApplicationTextureFormat::Rgba8Unorm,
        );
        pipeline.depth_format = Some(ApplicationTextureFormat::D32Float);
        pipeline.depth_test = false;
        pipeline.depth_write = false;
        pipeline.cull_mode = ApplicationCullMode::None;
        pipeline.vertex_attributes = vec![
            ApplicationVertexAttribute {
                semantic: "POSITION".into(),
                semantic_index: 0,
                format: ApplicationVertexFormat::Float2,
                input_slot: 0,
                byte_offset: 0,
                per_instance: false,
                instance_step_rate: 0,
            },
            ApplicationVertexAttribute {
                semantic: "COLOR".into(),
                semantic_index: 0,
                format: ApplicationVertexFormat::Float3,
                input_slot: 0,
                byte_offset: 8,
                per_instance: false,
                instance_step_rate: 0,
            },
        ];
        renderer
            .create_graphics_pipeline(&pipeline, &vertex_shader, &pixel_shader, 1)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's procedural sky pipeline")?;
        let vertex_buffer = renderer
            .create_buffer(
                ApplicationBufferDesc::new(VERTEX_BYTES, ApplicationBufferUsage::VERTEX)?
                    .with_memory(ApplicationBufferMemory::Upload),
            )
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating the game's sky vertex buffer")?;
        renderer
            .write_buffer(vertex_buffer, 0, &gradient_vertices())
            .map_err(ApplicationDiagnosticError::msg)
            .context("uploading the game's blue sky gradient")?;
        Ok(Self { pipeline, vertex_shader, pixel_shader, vertex_buffer })
    }

    pub(super) const fn pipeline(&self) -> &ApplicationGraphicsPipelineDesc {
        &self.pipeline
    }
    pub(super) const fn vertex_shader(&self) -> &ApplicationCompiledShader {
        &self.vertex_shader
    }
    pub(super) const fn pixel_shader(&self) -> &ApplicationCompiledShader {
        &self.pixel_shader
    }
    pub(super) const fn vertex_buffer(&self) -> nord_app::ApplicationGpuResourceHandle {
        self.vertex_buffer
    }
}

fn gradient_vertices() -> Vec<u8> {
    let top = [0.43_f32, 0.70, 0.94];
    let horizon = [0.78_f32, 0.87, 0.96];
    let points = [
        (-1.0_f32, -1.0_f32, horizon),
        (-1.0, 1.0, top),
        (1.0, 1.0, top),
        (-1.0, -1.0, horizon),
        (1.0, 1.0, top),
        (1.0, -1.0, horizon),
    ];
    let mut bytes = Vec::with_capacity(VERTEX_BYTES as usize);
    for (x, y, color) in points {
        for value in [x, y, color[0], color[1], color[2]] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}
