//! Game-owned first-person crosshair drawn over the 3D scene.

use nord_app::{
    ApplicationBufferDesc, ApplicationBufferMemory, ApplicationBufferUsage,
    ApplicationCompiledShader, ApplicationCullMode, ApplicationDiagnosticError,
    ApplicationErrorContext, ApplicationGraphicsPipelineDesc, ApplicationRenderer,
    ApplicationShaderStage, ApplicationTextureFormat, ApplicationVertexAttribute,
    ApplicationVertexFormat,
};

const BUFFER_BYTES: u64 = 192;
const BUFFER_CAPACITY: usize = 192;
const ARM_PIXELS: f32 = 9.0;
const GAP_PIXELS: f32 = 3.0;
const HALF_THICKNESS_PIXELS: f32 = 1.0;

pub(super) struct Crosshair {
    pipeline: ApplicationGraphicsPipelineDesc,
    vertex_shader: ApplicationCompiledShader,
    pixel_shader: ApplicationCompiledShader,
    vertex_buffer: nord_app::ApplicationGpuResourceHandle,
}

impl Crosshair {
    pub(super) fn create(
        renderer: &mut ApplicationRenderer,
        width: u32,
        height: u32,
    ) -> nord_app::ApplicationResult<Self> {
        let vertex_shader = ApplicationCompiledShader::from_precompiled(
            ApplicationShaderStage::Vertex,
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_crosshair.vs.dxil")).to_vec(),
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_crosshair.vs.reflection")).to_vec(),
            include_str!(concat!(env!("OUT_DIR"), "/nord_crosshair.vs.asm")).to_owned(),
        )
        .map_err(ApplicationDiagnosticError::msg)
        .context("loading the game's crosshair vertex shader")?;
        let pixel_shader = ApplicationCompiledShader::from_precompiled(
            ApplicationShaderStage::Pixel,
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_crosshair.ps.dxil")).to_vec(),
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_crosshair.ps.reflection")).to_vec(),
            include_str!(concat!(env!("OUT_DIR"), "/nord_crosshair.ps.asm")).to_owned(),
        )
        .map_err(ApplicationDiagnosticError::msg)
        .context("loading the game's crosshair pixel shader")?;

        let mut pipeline = ApplicationGraphicsPipelineDesc::new(
            vertex_shader.shader_id,
            pixel_shader.shader_id,
            ApplicationTextureFormat::Rgba8Unorm,
        )
        .with_blend_mode(nord_app::ApplicationBlendMode::InvertDestinationColor);
        pipeline.depth_format = Some(ApplicationTextureFormat::D32Float);
        pipeline.depth_test = false;
        pipeline.depth_write = false;
        pipeline.cull_mode = ApplicationCullMode::None;
        pipeline = pipeline.with_vertex_attribute(ApplicationVertexAttribute {
            semantic: "POSITION".into(),
            semantic_index: 0,
            format: ApplicationVertexFormat::Float2,
            input_slot: 0,
            byte_offset: 0,
            per_instance: false,
            instance_step_rate: 0,
        });
        renderer
            .create_graphics_pipeline(&pipeline, &vertex_shader, &pixel_shader, 1)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's screen-space crosshair pipeline")?;

        let vertex_buffer = renderer
            .create_buffer(
                ApplicationBufferDesc::new(BUFFER_BYTES, ApplicationBufferUsage::VERTEX)?
                    .with_memory(ApplicationBufferMemory::Upload),
            )
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating the game's crosshair vertex buffer")?;
        renderer
            .write_buffer(vertex_buffer, 0, &crosshair_vertices(width, height)?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("uploading the game's screen-centered crosshair")?;

        Ok(Self { pipeline, vertex_shader, pixel_shader, vertex_buffer })
    }

    pub(super) fn resize(
        &self,
        renderer: &mut ApplicationRenderer,
        width: u32,
        height: u32,
    ) -> nord_app::ApplicationResult<()> {
        renderer
            .write_buffer(self.vertex_buffer, 0, &crosshair_vertices(width, height)?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("resizing the game's screen-centered crosshair")
    }

    pub(super) const fn vertex_buffer(&self) -> nord_app::ApplicationGpuResourceHandle {
        self.vertex_buffer
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
}

fn crosshair_vertices(width: u32, height: u32) -> nord_app::ApplicationResult<Vec<u8>> {
    let sx = 2.0 / super::exact_surface_extent(width)?;
    let sy = 2.0 / super::exact_surface_extent(height)?;
    let mut vertices = Vec::with_capacity(BUFFER_CAPACITY);
    let arm = ARM_PIXELS;
    let gap = GAP_PIXELS;
    let t = HALF_THICKNESS_PIXELS;
    for rect in [(-arm, -gap, -t, t), (gap, arm, -t, t), (-t, t, -arm, -gap), (-t, t, gap, arm)] {
        let (left, right, bottom, top) = rect;
        for point in [
            (left, bottom),
            (right, bottom),
            (right, top),
            (left, bottom),
            (right, top),
            (left, top),
        ] {
            for value in [point.0 * sx, point.1 * sy] {
                vertices.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    Ok(vertices)
}

#[cfg(test)]
mod tests {
    use super::crosshair_vertices;

    #[test]
    fn crosshair_geometry_has_four_centered_arms_and_valid_buffer_size() {
        let vertices = crosshair_vertices(1280, 720).unwrap();
        assert_eq!(vertices.len(), 24 * 2 * std::mem::size_of::<f32>());
        let values = vertices
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert!(values.iter().all(|value| value.is_finite()));
        assert!(values.iter().any(|value| *value > 0.0));
        assert!(values.iter().any(|value| *value < 0.0));
    }
}
