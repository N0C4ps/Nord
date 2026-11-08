//! Small bitmap FPS readout drawn in the game composition pass.

use nord_app::{
    ApplicationBufferDesc, ApplicationBufferMemory, ApplicationBufferUsage,
    ApplicationCompiledShader, ApplicationCullMode, ApplicationDiagnosticError,
    ApplicationErrorContext, ApplicationGraphicsPipelineDesc, ApplicationRenderer,
    ApplicationShaderStage, ApplicationTextureFormat, ApplicationVertexAttribute,
    ApplicationVertexFormat,
};

const BUFFER_BYTES: u64 = 64 * 1024;
const PIXEL_SCALE: f32 = 2.0;
const LEFT_MARGIN: f32 = 14.0;
const TOP_MARGIN: f32 = 12.0;

pub(super) struct FpsHud {
    pipeline: ApplicationGraphicsPipelineDesc,
    vertex_shader: ApplicationCompiledShader,
    pixel_shader: ApplicationCompiledShader,
    vertex_buffer: nord_app::ApplicationGpuResourceHandle,
    last_label: Option<String>,
    last_extent: Option<(u32, u32)>,
}

impl FpsHud {
    pub(super) fn create(renderer: &mut ApplicationRenderer) -> nord_app::ApplicationResult<Self> {
        let vertex_shader = ApplicationCompiledShader::from_precompiled(
            ApplicationShaderStage::Vertex,
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_fps_hud.vs.dxil")).to_vec(),
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_fps_hud.vs.reflection")).to_vec(),
            include_str!(concat!(env!("OUT_DIR"), "/nord_fps_hud.vs.asm")).to_owned(),
        )
        .map_err(ApplicationDiagnosticError::msg)
        .context("loading the game's FPS HUD vertex shader")?;
        let pixel_shader = ApplicationCompiledShader::from_precompiled(
            ApplicationShaderStage::Pixel,
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_fps_hud.ps.dxil")).to_vec(),
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_fps_hud.ps.reflection")).to_vec(),
            include_str!(concat!(env!("OUT_DIR"), "/nord_fps_hud.ps.asm")).to_owned(),
        )
        .map_err(ApplicationDiagnosticError::msg)
        .context("loading the game's FPS HUD pixel shader")?;

        let mut pipeline = ApplicationGraphicsPipelineDesc::new(
            vertex_shader.shader_id,
            pixel_shader.shader_id,
            ApplicationTextureFormat::Rgba8Unorm,
        );
        pipeline.depth_format = None;
        pipeline.depth_test = false;
        pipeline.depth_write = false;
        pipeline.cull_mode = ApplicationCullMode::None;
        pipeline = pipeline
            .with_vertex_attribute(attribute("POSITION", ApplicationVertexFormat::Float2, 0))
            .with_vertex_attribute(attribute("COLOR", ApplicationVertexFormat::Float4, 8));
        renderer
            .create_graphics_pipeline(&pipeline, &vertex_shader, &pixel_shader, 1)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's FPS HUD pipeline")?;
        let vertex_buffer = renderer
            .create_buffer(
                ApplicationBufferDesc::new(BUFFER_BYTES, ApplicationBufferUsage::VERTEX)?
                    .with_memory(ApplicationBufferMemory::Upload),
            )
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating the game's FPS HUD geometry")?;

        Ok(Self {
            pipeline,
            vertex_shader,
            pixel_shader,
            vertex_buffer,
            last_label: None,
            last_extent: None,
        })
    }

    pub(super) fn update(
        &mut self,
        renderer: &mut ApplicationRenderer,
        width: u32,
        height: u32,
        fps: Option<f64>,
    ) -> nord_app::ApplicationResult<u32> {
        let fps = fps.filter(|value| value.is_finite() && *value >= 0.0).map_or_else(
            || "FPS: --".to_owned(),
            |value| format!("FPS: {}", value.round().min(99_999.0) as u32),
        );
        if self.last_label.as_deref() != Some(&fps) || self.last_extent != Some((width, height)) {
            let vertices = build_vertices(&fps, width, height)?;
            let byte_len = u64::try_from(vertices.len())
                .map_err(ApplicationDiagnosticError::msg)
                .context("checking the FPS HUD geometry size")?;
            nord_app::ensure!(
                byte_len <= BUFFER_BYTES,
                "FPS HUD geometry exceeded its fixed buffer"
            );
            renderer
                .write_buffer(self.vertex_buffer, 0, &vertices)
                .map_err(ApplicationDiagnosticError::msg)
                .context("updating the FPS counter geometry")?;
            self.last_label = Some(fps);
            self.last_extent = Some((width, height));
        }
        let label = self.last_label.as_deref().unwrap_or_default();
        u32::try_from(label.chars().map(glyph_pixel_count).sum::<usize>() * 12)
            .map_err(ApplicationDiagnosticError::msg)
            .context("checking the FPS HUD vertex count")
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

fn attribute(
    semantic: &str,
    format: ApplicationVertexFormat,
    byte_offset: u32,
) -> ApplicationVertexAttribute {
    ApplicationVertexAttribute {
        semantic: semantic.into(),
        semantic_index: 0,
        format,
        input_slot: 0,
        byte_offset,
        per_instance: false,
        instance_step_rate: 0,
    }
}

fn build_vertices(label: &str, width: u32, height: u32) -> nord_app::ApplicationResult<Vec<u8>> {
    nord_app::ensure!(width > 0 && height > 0, "FPS HUD requires a non-zero surface extent");
    let mut vertices = Vec::with_capacity(label.len() * 5 * 7 * 6 * 24);
    emit_label(&mut vertices, label, width, height, 1.0, 0.0, 0.0, 0.0);
    emit_label(&mut vertices, label, width, height, 0.0, 1.0, 1.0, 1.0);
    Ok(vertices)
}

#[allow(clippy::too_many_arguments)]
fn emit_label(
    vertices: &mut Vec<u8>,
    label: &str,
    width: u32,
    height: u32,
    offset: f32,
    red: f32,
    green: f32,
    blue: f32,
) {
    let mut cursor_x = LEFT_MARGIN;
    let origin_y = TOP_MARGIN;
    for character in label.chars() {
        let rows = glyph(character);
        for (row, bits) in rows.into_iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) == 0 {
                    continue;
                }
                let left = cursor_x + column as f32 * PIXEL_SCALE + offset;
                let top = origin_y + row as f32 * PIXEL_SCALE + offset;
                emit_quad(vertices, left, top, width, height, red, green, blue);
            }
        }
        cursor_x += 6.0 * PIXEL_SCALE;
    }
}

fn emit_quad(
    vertices: &mut Vec<u8>,
    left: f32,
    top: f32,
    width: u32,
    height: u32,
    red: f32,
    green: f32,
    blue: f32,
) {
    let right = left + PIXEL_SCALE;
    let bottom = top + PIXEL_SCALE;
    let w = width as f32;
    let h = height as f32;
    let points =
        [(left, top), (right, top), (right, bottom), (left, top), (right, bottom), (left, bottom)];
    for (x, y) in points {
        let values = [2.0 * x / w - 1.0, 1.0 - 2.0 * y / h, red, green, blue, 1.0];
        for value in values {
            vertices.extend_from_slice(&value.to_le_bytes());
        }
    }
}

fn glyph_pixel_count(character: char) -> usize {
    glyph(character).into_iter().map(|row| row.count_ones() as usize).sum()
}

fn glyph(character: char) -> [u8; 7] {
    match character {
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        ':' => [0, 0b00100, 0b00100, 0, 0b00100, 0b00100, 0],
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110],
        '6' => [0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110],
        _ => [0; 7],
    }
}

#[cfg(test)]
mod tests {
    use super::build_vertices;

    #[test]
    fn fps_readout_geometry_is_non_empty_and_finite() {
        let bytes = build_vertices("FPS: 01234", 1280, 720).expect("valid HUD geometry");
        assert!(!bytes.is_empty());
        assert_eq!(bytes.len() % 24, 0);
        for value in
            bytes.chunks_exact(4).map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
        {
            assert!(value.is_finite());
        }
    }
}
