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
const ARM_PIXELS: f32 = 16.0;
const GAP_PIXELS: f32 = 0.0;
const HALF_THICKNESS_PIXELS: f32 = 2.0;

pub(super) struct Crosshair {
    pipeline: ApplicationGraphicsPipelineDesc,
    vertex_shader: ApplicationCompiledShader,
    pixel_shader: ApplicationCompiledShader,
    blit_pipeline: ApplicationGraphicsPipelineDesc,
    blit_vertex_shader: ApplicationCompiledShader,
    blit_pixel_shader: ApplicationCompiledShader,
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
        let blit_vertex_shader = ApplicationCompiledShader::from_precompiled(
            ApplicationShaderStage::Vertex,
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_crosshair_blit.vs.dxil")).to_vec(),
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_crosshair_blit.vs.reflection")).to_vec(),
            include_str!(concat!(env!("OUT_DIR"), "/nord_crosshair_blit.vs.asm")).to_owned(),
        )
        .map_err(ApplicationDiagnosticError::msg)
        .context("loading the game's screen-composition vertex shader")?;
        let blit_pixel_shader = ApplicationCompiledShader::from_precompiled(
            ApplicationShaderStage::Pixel,
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_crosshair_blit.ps.dxil")).to_vec(),
            include_bytes!(concat!(env!("OUT_DIR"), "/nord_crosshair_blit.ps.reflection")).to_vec(),
            include_str!(concat!(env!("OUT_DIR"), "/nord_crosshair_blit.ps.asm")).to_owned(),
        )
        .map_err(ApplicationDiagnosticError::msg)
        .context("loading the game's screen-composition pixel shader")?;

        let mut pipeline = ApplicationGraphicsPipelineDesc::new(
            vertex_shader.shader_id,
            pixel_shader.shader_id,
            ApplicationTextureFormat::Rgba8Unorm,
        );
        pipeline.depth_format = None;
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

        let mut blit_pipeline = ApplicationGraphicsPipelineDesc::new(
            blit_vertex_shader.shader_id,
            blit_pixel_shader.shader_id,
            ApplicationTextureFormat::Rgba8Unorm,
        );
        blit_pipeline.depth_format = None;
        blit_pipeline.depth_test = false;
        blit_pipeline.depth_write = false;
        blit_pipeline.cull_mode = ApplicationCullMode::None;
        renderer
            .create_graphics_pipeline(&pipeline, &vertex_shader, &pixel_shader, 1)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's screen-space crosshair pipeline")?;
        renderer
            .create_graphics_pipeline(&blit_pipeline, &blit_vertex_shader, &blit_pixel_shader, 1)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's scene-composition pipeline")?;

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

        Ok(Self {
            pipeline,
            vertex_shader,
            pixel_shader,
            blit_pipeline,
            blit_vertex_shader,
            blit_pixel_shader,
            vertex_buffer,
        })
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

    pub(super) const fn blit_pipeline(&self) -> &ApplicationGraphicsPipelineDesc {
        &self.blit_pipeline
    }

    pub(super) const fn blit_vertex_shader(&self) -> &ApplicationCompiledShader {
        &self.blit_vertex_shader
    }

    pub(super) const fn blit_pixel_shader(&self) -> &ApplicationCompiledShader {
        &self.blit_pixel_shader
    }
}

#[cfg(test)]
fn contrast_color_srgb(background: [f32; 3]) -> [f32; 3] {
    let linear = background.map(|channel| {
        if channel <= 0.04045 { channel / 12.92 } else { ((channel + 0.055) / 1.055).powf(2.4) }
    });
    let luminance = linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722;
    let chroma = background.iter().copied().fold(f32::NEG_INFINITY, f32::max)
        - background.iter().copied().fold(f32::INFINITY, f32::min);
    if chroma < 0.08 {
        let neutral = if luminance < 0.74 { 1.0 } else { 0.0 };
        return [neutral; 3];
    }
    let mut complement = linear.map(|channel| 1.0 - channel);
    let complement_luminance =
        complement[0] * 0.2126 + complement[1] * 0.7152 + complement[2] * 0.0722;
    let contrast =
        (luminance.max(complement_luminance) + 0.05) / (luminance.min(complement_luminance) + 0.05);
    if contrast < 4.5 {
        let background_is_light = luminance >= 0.18;
        let target_luminance = if background_is_light {
            ((luminance + 0.05) / 4.5 - 0.05).clamp(0.0, 1.0)
        } else {
            (4.5 * (luminance + 0.05) - 0.05).clamp(0.0, 1.0)
        };
        if target_luminance < complement_luminance {
            let scale = target_luminance / complement_luminance.max(0.0001);
            complement.iter_mut().for_each(|channel| *channel *= scale);
        } else {
            let lift = (target_luminance - complement_luminance)
                / (1.0 - complement_luminance).max(0.0001);
            complement.iter_mut().for_each(|channel| {
                *channel += (1.0 - *channel) * lift.clamp(0.0, 1.0);
            });
        }
    }
    complement.map(|inverse| {
        if inverse <= 0.003_130_8 {
            12.92 * inverse
        } else {
            1.055 * inverse.powf(1.0 / 2.4) - 0.055
        }
    })
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
    use super::{contrast_color_srgb, crosshair_vertices};

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

    #[test]
    fn crosshair_uses_neutral_white_on_medium_gray() {
        assert_eq!(contrast_color_srgb([0.5, 0.5, 0.5]), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn crosshair_complements_blue_sky_to_gold() {
        let color = contrast_color_srgb([0.43, 0.70, 0.94]);
        assert!(color[0] > color[1] && color[1] > color[2]);
        let background_luminance = 0.43_f32.powf(2.4) * 0.2126
            + ((0.70 + 0.055) / 1.055_f32).powf(2.4) * 0.7152
            + ((0.94 + 0.055) / 1.055_f32).powf(2.4) * 0.0722;
        let crosshair_luminance = color[0].powf(2.4) * 0.2126
            + ((color[1] + 0.055) / 1.055_f32).powf(2.4) * 0.7152
            + ((color[2] + 0.055) / 1.055_f32).powf(2.4) * 0.0722;
        assert!(
            (background_luminance.max(crosshair_luminance) + 0.05)
                / (background_luminance.min(crosshair_luminance) + 0.05)
                >= 4.49
        );
    }

    #[test]
    fn crosshair_complements_green_to_magenta() {
        let color = contrast_color_srgb([0.2, 0.7, 0.2]);
        assert!(color[0] > color[1] && color[2] > color[1]);
    }

    #[test]
    fn crosshair_does_not_treat_dark_wood_as_gray() {
        let color = contrast_color_srgb([0.3, 0.2, 0.1]);
        assert!(color[2] > color[0]);
        assert!(color[0] < 1.0 && color[1] < 1.0);
    }
}
