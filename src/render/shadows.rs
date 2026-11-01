//! Game-specific directional shadow-map setup built from Nord's render facilities.

use nord_app::{
    ApplicationCompiledShader, ApplicationDescriptorHandle, ApplicationDiagnosticError,
    ApplicationErrorContext, ApplicationGraphicsPipelineDesc, ApplicationPipelineDescriptorBinding,
    ApplicationRenderTargetDesc, ApplicationRenderer, ApplicationShaderBindingKind,
    ApplicationShaderStage, ApplicationTextureFormat, ApplicationVertexAttribute,
    ApplicationVertexFormat,
};

pub(super) const SHADOW_MAP_RESOLUTION: u32 = 2048;
const SHADOW_MAP_EXTENT: f32 = 60.0;
const LIGHT_NEAR: f32 = 0.1;
const LIGHT_FAR: f32 = 220.0;

#[derive(Clone, Copy)]
pub(super) struct ShadowMapResources {
    pub(super) color: nord_app::ApplicationGpuResourceHandle,
    pub(super) color_view: ApplicationDescriptorHandle,
    pub(super) depth: nord_app::ApplicationGpuResourceHandle,
    pub(super) depth_view: ApplicationDescriptorHandle,
}

/// One directional light map with its own raster depth attachment and sampled depth values.
pub(super) struct DirectionalShadowMap {
    resources: ShadowMapResources,
    sampled_binding: ApplicationPipelineDescriptorBinding,
    pipeline: ApplicationGraphicsPipelineDesc,
    vertex_shader: ApplicationCompiledShader,
    pixel_shader: ApplicationCompiledShader,
}

impl DirectionalShadowMap {
    pub(super) fn create(renderer: &mut ApplicationRenderer) -> nord_app::ApplicationResult<Self> {
        let target = renderer
            .create_render_target(ApplicationRenderTargetDesc::new(
                SHADOW_MAP_RESOLUTION,
                SHADOW_MAP_RESOLUTION,
                Some(ApplicationTextureFormat::R32Float),
                Some(ApplicationTextureFormat::D32Float),
            )?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating the directional light's shadow-map targets")?;
        let color = target.color.expect("shadow color target was requested");
        let depth = target.depth.expect("shadow depth target was requested");
        let color_view = renderer
            .create_render_target_view(color)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the directional shadow depth-value attachment")?;
        let depth_view = renderer
            .create_depth_stencil_view(depth)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the directional shadow raster depth attachment")?;
        let sampled_view = renderer
            .create_texture_srv(color)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the sampled directional shadow map")?;
        let (vertex_shader, pixel_shader) = load_validated_shader_program()?;
        let mut pipeline = ApplicationGraphicsPipelineDesc::new(
            vertex_shader.shader_id,
            pixel_shader.shader_id,
            ApplicationTextureFormat::R32Float,
        );
        pipeline.depth_format = Some(ApplicationTextureFormat::D32Float);
        pipeline.depth_test = true;
        pipeline.depth_write = true;
        pipeline.alpha_blending = false;
        pipeline = pipeline
            .with_vertex_attribute(vertex_attribute("POSITION", ApplicationVertexFormat::Float3, 0))
            .with_vertex_attribute(instance_vertex_attribute(
                "INSTANCE_TRANSLATION",
                ApplicationVertexFormat::Float3,
                0,
            ));
        renderer
            .create_graphics_pipeline(&pipeline, &vertex_shader, &pixel_shader, 1)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the directional shadow depth pipeline")?;
        Ok(Self {
            resources: ShadowMapResources { color, color_view, depth, depth_view },
            sampled_binding: ApplicationPipelineDescriptorBinding {
                kind: ApplicationShaderBindingKind::SampledResource,
                register: 3,
                space: 0,
                descriptor: sampled_view,
            },
            pipeline,
            vertex_shader,
            pixel_shader,
        })
    }

    pub(super) const fn resolution() -> u32 {
        SHADOW_MAP_RESOLUTION
    }

    pub(super) const fn resources(&self) -> ShadowMapResources {
        self.resources
    }

    pub(super) const fn sampled_binding(&self) -> ApplicationPipelineDescriptorBinding {
        self.sampled_binding
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

    pub(super) fn shutdown(
        &self,
        renderer: &mut ApplicationRenderer,
    ) -> nord_app::ApplicationResult<()> {
        renderer
            .destroy_descriptor(self.sampled_binding.descriptor)
            .map_err(ApplicationDiagnosticError::msg)
            .context("retiring the directional shadow map sampled view")?;
        for resource in [self.resources.color, self.resources.depth] {
            renderer
                .destroy_resource(resource)
                .map_err(ApplicationDiagnosticError::msg)
                .context("retiring a directional shadow map target")?;
        }
        Ok(())
    }
}

pub(super) fn directional_light_view_projection() -> Option<nord_app::ApplicationMat4> {
    let direction_to_light = nord_app::ApplicationVec3::new(-0.45, 0.72, -0.52).try_normalize()?;
    let eye = direction_to_light * 100.0;
    let view = nord_app::ApplicationMat4::look_to_rh(
        eye,
        -direction_to_light,
        nord_app::ApplicationVec3::Y,
    )?;
    let extent = SHADOW_MAP_EXTENT;
    let projection = nord_app::ApplicationMat4::from_cols(
        nord_app::ApplicationVec4::new(1.0 / extent, 0.0, 0.0, 0.0),
        nord_app::ApplicationVec4::new(0.0, 1.0 / extent, 0.0, 0.0),
        nord_app::ApplicationVec4::new(0.0, 0.0, 1.0 / (LIGHT_NEAR - LIGHT_FAR), 0.0),
        nord_app::ApplicationVec4::new(0.0, 0.0, LIGHT_NEAR / (LIGHT_NEAR - LIGHT_FAR), 1.0),
    );
    Some(projection * view)
}

fn load_validated_shader_program()
-> nord_app::ApplicationResult<(ApplicationCompiledShader, ApplicationCompiledShader)> {
    let vertex = ApplicationCompiledShader::from_precompiled(
        ApplicationShaderStage::Vertex,
        include_bytes!(concat!(env!("OUT_DIR"), "/nord_directional_shadow.vs.dxil")).to_vec(),
        include_bytes!(concat!(env!("OUT_DIR"), "/nord_directional_shadow.vs.reflection")).to_vec(),
        include_str!(concat!(env!("OUT_DIR"), "/nord_directional_shadow.vs.asm")).to_owned(),
    )
    .map_err(ApplicationDiagnosticError::msg)
    .context("loading the embedded directional shadow vertex shader")?;
    let pixel = ApplicationCompiledShader::from_precompiled(
        ApplicationShaderStage::Pixel,
        include_bytes!(concat!(env!("OUT_DIR"), "/nord_directional_shadow.ps.dxil")).to_vec(),
        include_bytes!(concat!(env!("OUT_DIR"), "/nord_directional_shadow.ps.reflection")).to_vec(),
        include_str!(concat!(env!("OUT_DIR"), "/nord_directional_shadow.ps.asm")).to_owned(),
    )
    .map_err(ApplicationDiagnosticError::msg)
    .context("loading the embedded directional shadow pixel shader")?;
    require_camera_binding(&vertex)?;
    Ok((vertex, pixel))
}

fn require_camera_binding(shader: &ApplicationCompiledShader) -> nord_app::ApplicationResult<()> {
    if shader.stage == ApplicationShaderStage::Vertex
        && shader.reflection.bindings().iter().any(|binding| {
            binding.name == "CameraData"
                && binding.kind == ApplicationShaderBindingKind::ConstantBuffer
                && binding.register == 0
                && binding.space == 0
                && binding.count == 1
        })
    {
        Ok(())
    } else {
        Err(ApplicationDiagnosticError::msg(
            "directional shadow shader contract mismatch: CameraData must be at b0 in the vertex stage",
        ))
    }
}

fn vertex_attribute(
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

fn instance_vertex_attribute(
    semantic: &str,
    format: ApplicationVertexFormat,
    byte_offset: u32,
) -> ApplicationVertexAttribute {
    ApplicationVertexAttribute {
        semantic: semantic.into(),
        semantic_index: 0,
        format,
        input_slot: 1,
        byte_offset,
        per_instance: true,
        instance_step_rate: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::directional_light_view_projection;
    use crate::render::visibility::GameScene;
    use nord_app::{ApplicationAabb3 as Aabb3, ApplicationMat4 as Mat4, ApplicationVec3 as Vec3};

    #[test]
    fn directional_light_frustum_contains_the_complete_demo_caster_grid() {
        let bounds = Aabb3::new(Vec3::splat(-1.0), Vec3::splat(1.0))
            .expect("unit test cube bounds should be valid");
        let model = Mat4::from_scale(Vec3::splat(0.9));
        let scene = GameScene::demo_grid(bounds, model);
        let mut visible_mask = Vec::new();
        let mut draw_ranges = Vec::new();
        let light_matrix = directional_light_view_projection()
            .expect("directional light projection should be valid");

        let metrics = scene.update_visibility(light_matrix, &mut visible_mask, &mut draw_ranges);

        assert_eq!(metrics.total_instances, 1681);
        assert_eq!(metrics.visible_instances, metrics.total_instances);
        assert!(metrics.instanced_draw_calls > 0);
    }
}
