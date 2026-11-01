//! Game-owned metallic-roughness material and its embedded forward shader.

use nord_app::{
    ApplicationBufferDesc, ApplicationBufferMemory, ApplicationBufferUsage,
    ApplicationCompiledShader, ApplicationDescriptorHandle, ApplicationDiagnosticError,
    ApplicationErrorContext, ApplicationGpuResourceHandle, ApplicationGraphicsPipelineDesc,
    ApplicationPipelineDescriptorBinding, ApplicationRenderer, ApplicationSamplerAddressMode,
    ApplicationSamplerDesc, ApplicationSamplerFilter, ApplicationShaderBindingKind,
    ApplicationShaderStage, ApplicationTextureDesc, ApplicationTextureFormat,
    ApplicationTextureUsage, ApplicationVertexAttribute, ApplicationVertexFormat,
};

const MATERIAL_CONSTANT_BYTES: u64 = 256;
const MATERIAL_CONSTANT_REGISTER: u32 = 1;
const BASE_COLOR_REGISTER: u32 = 0;
const METALLIC_ROUGHNESS_REGISTER: u32 = 1;
const NORMAL_REGISTER: u32 = 2;
const SAMPLER_REGISTER: u32 = 0;
const TEXTURE_SIZE: u32 = 64;
const TEXTURE_MIP_LEVELS: u16 = 7;

/// Game material factors follow glTF metallic-roughness conventions.
///
/// `base_color_linear` is scene-linear RGB. Texture RGB is sampled through an
/// sRGB view and decoded before multiplication; metallic/roughness and normal
/// data use linear UNORM views.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PbrMaterialDesc {
    base_color_linear: [f32; 3],
    metallic_factor: f32,
    roughness_factor: f32,
    normal_scale: f32,
    exposure: f32,
}

impl PbrMaterialDesc {
    /// Create an opaque metallic-roughness material in linear-light space.
    ///
    /// # Errors
    /// Returns an error when a factor is non-finite or outside its supported range.
    pub(super) fn new(
        base_color_linear: [f32; 3],
        metallic_factor: f32,
        roughness_factor: f32,
        normal_scale: f32,
        exposure: f32,
    ) -> nord_app::ApplicationResult<Self> {
        if base_color_linear
            .iter()
            .any(|component| !component.is_finite() || !(0.0..=1.0).contains(component))
            || !metallic_factor.is_finite()
            || !(0.0..=1.0).contains(&metallic_factor)
            || !roughness_factor.is_finite()
            || !(0.0..=1.0).contains(&roughness_factor)
            || !normal_scale.is_finite()
            || !(0.0..=4.0).contains(&normal_scale)
            || !exposure.is_finite()
            || !(0.0..=16.0).contains(&exposure)
        {
            return Err(ApplicationDiagnosticError::msg(
                "PBR material factors must be finite and within their documented ranges",
            ));
        }
        Ok(Self { base_color_linear, metallic_factor, roughness_factor, normal_scale, exposure })
    }
}

/// Game material shaders, factors, sampled maps and descriptors.
pub(super) struct PbrMaterial {
    pipeline: ApplicationGraphicsPipelineDesc,
    vertex_shader: ApplicationCompiledShader,
    pixel_shader: ApplicationCompiledShader,
    constants: ApplicationGpuResourceHandle,
    constants_view: ApplicationDescriptorHandle,
    texture_resources: [ApplicationGpuResourceHandle; 3],
    texture_views: [ApplicationDescriptorHandle; 3],
    sampler: ApplicationDescriptorHandle,
    bindings: [ApplicationPipelineDescriptorBinding; 5],
}

impl PbrMaterial {
    /// Load embedded shaders, create material maps and build an opaque PBR pipeline.
    ///
    /// # Errors
    /// Returns diagnostics from shader validation, GPU resource upload or pipeline creation.
    pub(super) fn load(
        renderer: &mut ApplicationRenderer,
        descriptor: PbrMaterialDesc,
    ) -> nord_app::ApplicationResult<Self> {
        let (vertex_shader, pixel_shader) = load_validated_shader_program()?;
        let pipeline = create_pbr_pipeline(renderer, &vertex_shader, &pixel_shader)?;

        let constants = renderer
            .create_buffer(
                ApplicationBufferDesc::new(
                    MATERIAL_CONSTANT_BYTES,
                    ApplicationBufferUsage::CONSTANT,
                )?
                .with_memory(ApplicationBufferMemory::Upload),
            )
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating the game's PBR material constants")?;
        let constants_view = renderer
            .create_constant_buffer_view(constants)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's PBR material constant binding")?;
        renderer
            .write_buffer(constants, 0, &encode_material_constants(descriptor))
            .map_err(ApplicationDiagnosticError::msg)
            .context("writing the game's linear PBR material factors")?;

        let (texture_resources, texture_views, sampler) = create_material_textures(renderer)?;
        let bindings = create_material_bindings(constants_view, texture_views, sampler);

        Ok(Self {
            pipeline,
            vertex_shader,
            pixel_shader,
            constants,
            constants_view,
            texture_resources,
            texture_views,
            sampler,
            bindings,
        })
    }

    #[must_use]
    pub(super) const fn pipeline(&self) -> &ApplicationGraphicsPipelineDesc {
        &self.pipeline
    }

    #[must_use]
    pub(super) const fn vertex_shader(&self) -> &ApplicationCompiledShader {
        &self.vertex_shader
    }

    #[must_use]
    pub(super) const fn pixel_shader(&self) -> &ApplicationCompiledShader {
        &self.pixel_shader
    }

    #[must_use]
    pub(super) const fn bindings(&self) -> &[ApplicationPipelineDescriptorBinding; 5] {
        &self.bindings
    }

    #[must_use]
    pub(super) const fn constants_resource(&self) -> ApplicationGpuResourceHandle {
        self.constants
    }

    #[must_use]
    pub(super) const fn texture_resources(&self) -> [ApplicationGpuResourceHandle; 3] {
        self.texture_resources
    }

    /// Release material descriptors and textures after queue idle.
    ///
    /// # Errors
    /// Returns an error if a native descriptor or resource cannot be retired.
    pub(super) fn shutdown(
        &self,
        renderer: &mut ApplicationRenderer,
    ) -> nord_app::ApplicationResult<()> {
        for descriptor in [
            self.constants_view,
            self.texture_views[0],
            self.texture_views[1],
            self.texture_views[2],
            self.sampler,
        ] {
            renderer
                .destroy_descriptor(descriptor)
                .map_err(ApplicationDiagnosticError::msg)
                .context("retiring a game PBR material descriptor")?;
        }
        for resource in [
            self.constants,
            self.texture_resources[0],
            self.texture_resources[1],
            self.texture_resources[2],
        ] {
            renderer
                .destroy_resource(resource)
                .map_err(ApplicationDiagnosticError::msg)
                .context("retiring a game PBR material resource")?;
        }
        Ok(())
    }
}

fn load_validated_shader_program()
-> nord_app::ApplicationResult<(ApplicationCompiledShader, ApplicationCompiledShader)> {
    let (vertex_shader, pixel_shader) = load_shader_program()?;
    require_binding(
        &vertex_shader,
        ApplicationShaderStage::Vertex,
        ApplicationShaderBindingKind::ConstantBuffer,
        "CameraData",
        0,
    )?;
    for (name, kind, register) in [
        ("MaterialData", ApplicationShaderBindingKind::ConstantBuffer, MATERIAL_CONSTANT_REGISTER),
        ("baseColorTexture", ApplicationShaderBindingKind::SampledResource, BASE_COLOR_REGISTER),
        (
            "metallicRoughnessTexture",
            ApplicationShaderBindingKind::SampledResource,
            METALLIC_ROUGHNESS_REGISTER,
        ),
        ("normalTexture", ApplicationShaderBindingKind::SampledResource, NORMAL_REGISTER),
        ("materialSampler", ApplicationShaderBindingKind::Sampler, SAMPLER_REGISTER),
    ] {
        require_binding(&pixel_shader, ApplicationShaderStage::Pixel, kind, name, register)?;
    }
    Ok((vertex_shader, pixel_shader))
}

fn create_pbr_pipeline(
    renderer: &mut ApplicationRenderer,
    vertex_shader: &ApplicationCompiledShader,
    pixel_shader: &ApplicationCompiledShader,
) -> nord_app::ApplicationResult<ApplicationGraphicsPipelineDesc> {
    let mut pipeline = ApplicationGraphicsPipelineDesc::new(
        vertex_shader.shader_id,
        pixel_shader.shader_id,
        ApplicationTextureFormat::Rgba8Unorm,
    );
    pipeline.depth_format = Some(ApplicationTextureFormat::D32Float);
    pipeline.depth_test = true;
    pipeline.depth_write = true;
    pipeline.alpha_blending = false;
    pipeline = pipeline
        .with_vertex_attribute(vertex_attribute("POSITION", ApplicationVertexFormat::Float3, 0))
        .with_vertex_attribute(vertex_attribute("NORMAL", ApplicationVertexFormat::Float3, 12))
        .with_vertex_attribute(vertex_attribute("TANGENT", ApplicationVertexFormat::Float4, 24))
        .with_vertex_attribute(vertex_attribute("TEXCOORD", ApplicationVertexFormat::Float2, 40));
    renderer
        .create_graphics_pipeline(&pipeline, vertex_shader, pixel_shader, 1)
        .map_err(ApplicationDiagnosticError::msg)
        .context("creating the game's opaque metallic-roughness pipeline")?;
    Ok(pipeline)
}

fn create_material_textures(
    renderer: &mut ApplicationRenderer,
) -> nord_app::ApplicationResult<(
    [ApplicationGpuResourceHandle; 3],
    [ApplicationDescriptorHandle; 3],
    ApplicationDescriptorHandle,
)> {
    let mip_chains = build_procedural_material_maps();
    let (base_color, base_color_view) =
        create_texture(renderer, ApplicationTextureFormat::Rgba8Srgb, &mip_chains[0])
            .context("creating the game's sRGB base-color map")?;
    let (metallic_roughness, metallic_roughness_view) =
        create_texture(renderer, ApplicationTextureFormat::Rgba8Unorm, &mip_chains[1])
            .context("creating the game's linear metallic-roughness map")?;
    let (normal, normal_view) =
        create_texture(renderer, ApplicationTextureFormat::Rgba8Unorm, &mip_chains[2])
            .context("creating the game's tangent-space normal map")?;
    let sampler = renderer
        .create_sampler(ApplicationSamplerDesc {
            filter: ApplicationSamplerFilter::Anisotropic,
            address_u: ApplicationSamplerAddressMode::Repeat,
            address_v: ApplicationSamplerAddressMode::Repeat,
            address_w: ApplicationSamplerAddressMode::Repeat,
            max_anisotropy: 8,
            min_lod: 0.0,
            max_lod: f32::from(TEXTURE_MIP_LEVELS - 1),
        })
        .map_err(ApplicationDiagnosticError::msg)
        .context("creating the game's anisotropic PBR texture sampler")?;
    Ok((
        [base_color, metallic_roughness, normal],
        [base_color_view, metallic_roughness_view, normal_view],
        sampler,
    ))
}

fn create_material_bindings(
    constants_view: ApplicationDescriptorHandle,
    texture_views: [ApplicationDescriptorHandle; 3],
    sampler: ApplicationDescriptorHandle,
) -> [ApplicationPipelineDescriptorBinding; 5] {
    [
        descriptor_binding(
            ApplicationShaderBindingKind::ConstantBuffer,
            MATERIAL_CONSTANT_REGISTER,
            constants_view,
        ),
        descriptor_binding(
            ApplicationShaderBindingKind::SampledResource,
            BASE_COLOR_REGISTER,
            texture_views[0],
        ),
        descriptor_binding(
            ApplicationShaderBindingKind::SampledResource,
            METALLIC_ROUGHNESS_REGISTER,
            texture_views[1],
        ),
        descriptor_binding(
            ApplicationShaderBindingKind::SampledResource,
            NORMAL_REGISTER,
            texture_views[2],
        ),
        descriptor_binding(ApplicationShaderBindingKind::Sampler, SAMPLER_REGISTER, sampler),
    ]
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

fn descriptor_binding(
    kind: ApplicationShaderBindingKind,
    register: u32,
    descriptor: ApplicationDescriptorHandle,
) -> ApplicationPipelineDescriptorBinding {
    ApplicationPipelineDescriptorBinding { kind, register, space: 0, descriptor }
}

fn require_binding(
    shader: &ApplicationCompiledShader,
    stage: ApplicationShaderStage,
    kind: ApplicationShaderBindingKind,
    name: &str,
    register: u32,
) -> nord_app::ApplicationResult<()> {
    let matches = shader.stage == stage
        && shader.reflection.bindings().iter().any(|binding| {
            binding.name == name
                && binding.kind == kind
                && binding.register == register
                && binding.space == 0
                && binding.count == 1
        });
    if matches {
        Ok(())
    } else {
        Err(ApplicationDiagnosticError::msg(format!(
            "PBR shader contract mismatch: {name} must have kind {kind:?} at register {register}, space0, in {stage:?} stage"
        )))
    }
}

fn load_shader_program()
-> nord_app::ApplicationResult<(ApplicationCompiledShader, ApplicationCompiledShader)> {
    let vertex = ApplicationCompiledShader::from_precompiled(
        ApplicationShaderStage::Vertex,
        include_bytes!(concat!(env!("OUT_DIR"), "/nord_pbr_forward.vs.dxil")).to_vec(),
        include_bytes!(concat!(env!("OUT_DIR"), "/nord_pbr_forward.vs.reflection")).to_vec(),
        include_str!(concat!(env!("OUT_DIR"), "/nord_pbr_forward.vs.asm")).to_owned(),
    )
    .map_err(ApplicationDiagnosticError::msg)
    .context("loading the game's embedded PBR vertex shader")?;
    let pixel = ApplicationCompiledShader::from_precompiled(
        ApplicationShaderStage::Pixel,
        include_bytes!(concat!(env!("OUT_DIR"), "/nord_pbr_forward.ps.dxil")).to_vec(),
        include_bytes!(concat!(env!("OUT_DIR"), "/nord_pbr_forward.ps.reflection")).to_vec(),
        include_str!(concat!(env!("OUT_DIR"), "/nord_pbr_forward.ps.asm")).to_owned(),
    )
    .map_err(ApplicationDiagnosticError::msg)
    .context("loading the game's embedded PBR pixel shader")?;
    Ok((vertex, pixel))
}

fn encode_material_constants(descriptor: PbrMaterialDesc) -> [u8; 256] {
    let mut bytes = [0_u8; 256];
    let values = [
        descriptor.base_color_linear[0],
        descriptor.base_color_linear[1],
        descriptor.base_color_linear[2],
        1.0,
        descriptor.metallic_factor,
        descriptor.roughness_factor,
        0.0,
        0.0,
        descriptor.normal_scale,
        descriptor.exposure,
        0.0,
        0.0,
    ];
    for (index, value) in values.into_iter().enumerate() {
        let offset = index * std::mem::size_of::<f32>();
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn create_texture(
    renderer: &mut ApplicationRenderer,
    format: ApplicationTextureFormat,
    mip_chain: &[Vec<u8>],
) -> nord_app::ApplicationResult<(ApplicationGpuResourceHandle, ApplicationDescriptorHandle)> {
    let mip_levels = u16::try_from(mip_chain.len())
        .map_err(ApplicationDiagnosticError::msg)
        .context("validating a game material texture mip count")?;
    let description = ApplicationTextureDesc::new(
        TEXTURE_SIZE,
        TEXTURE_SIZE,
        format,
        ApplicationTextureUsage::SAMPLED.union(ApplicationTextureUsage::COPY_DESTINATION),
    )?
    .with_mip_levels(mip_levels)?;
    let resource = renderer
        .create_texture(description)
        .map_err(ApplicationDiagnosticError::msg)
        .context("allocating a game PBR texture")?;
    let mut width = TEXTURE_SIZE;
    for (level, pixels) in mip_chain.iter().enumerate() {
        let level = u16::try_from(level)
            .map_err(ApplicationDiagnosticError::msg)
            .context("checking a game material texture mip index")?;
        let row_pitch = usize::try_from(width)
            .ok()
            .and_then(|value| value.checked_mul(4))
            .ok_or_else(|| ApplicationDiagnosticError::msg("PBR texture row pitch overflowed"))?;
        renderer
            .upload_texture(resource, level, 0, row_pitch, pixels)
            .map_err(ApplicationDiagnosticError::msg)
            .context("uploading a game PBR texture mip")?;
        width = (width / 2).max(1);
    }
    let view = renderer
        .create_texture_srv(resource)
        .map_err(ApplicationDiagnosticError::msg)
        .context("creating a game PBR texture view")?;
    Ok((resource, view))
}

fn build_procedural_material_maps() -> [Vec<Vec<u8>>; 3] {
    let mut base_color = Vec::with_capacity((TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize);
    let mut metallic_roughness = Vec::with_capacity((TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize);
    let mut normal = Vec::with_capacity((TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize);
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let checker = ((x / 8) + (y / 8)) % 2 == 0;
            let color_linear = if checker { [0.88, 0.84, 0.78] } else { [1.0, 0.97, 0.90] };
            base_color.extend(color_linear.map(encode_srgb_byte));
            base_color.push(255);

            let u = texture_coordinate(x);
            let v = texture_coordinate(y);
            let roughness = 0.40 + 0.18 * (u * std::f32::consts::TAU * 3.0).sin().abs();
            let metallic = if (x / 16) % 2 == 0 { 0.92 } else { 0.62 };
            metallic_roughness.extend([
                255,
                linear_unorm_byte(roughness),
                linear_unorm_byte(metallic),
                255,
            ]);

            let tangent_x = 0.16 * (u * std::f32::consts::TAU * 4.0).sin();
            let tangent_y = 0.16 * (v * std::f32::consts::TAU * 4.0).cos();
            let tangent_z = (1.0 - tangent_x * tangent_x - tangent_y * tangent_y).sqrt();
            normal.extend([
                linear_unorm_byte(tangent_x * 0.5 + 0.5),
                linear_unorm_byte(tangent_y * 0.5 + 0.5),
                linear_unorm_byte(tangent_z * 0.5 + 0.5),
                255,
            ]);
        }
    }
    [
        generate_mips(base_color, TextureMipKind::SrgbColor),
        generate_mips(metallic_roughness, TextureMipKind::LinearData),
        generate_mips(normal, TextureMipKind::Normal),
    ]
}

#[derive(Clone, Copy)]
enum TextureMipKind {
    SrgbColor,
    LinearData,
    Normal,
}

fn generate_mips(mut pixels: Vec<u8>, kind: TextureMipKind) -> Vec<Vec<u8>> {
    let mut levels = Vec::with_capacity(usize::from(TEXTURE_MIP_LEVELS));
    let mut width = TEXTURE_SIZE;
    levels.push(pixels.clone());
    while width > 1 {
        let next_width = width / 2;
        let mut next = Vec::with_capacity((next_width * next_width * 4) as usize);
        for y in 0..next_width {
            for x in 0..next_width {
                let offsets = [
                    (((y * 2) * width + x * 2) * 4) as usize,
                    (((y * 2) * width + x * 2 + 1) * 4) as usize,
                    (((y * 2 + 1) * width + x * 2) * 4) as usize,
                    (((y * 2 + 1) * width + x * 2 + 1) * 4) as usize,
                ];
                match kind {
                    TextureMipKind::Normal => {
                        let mut average = [0.0_f32; 3];
                        for offset in offsets {
                            for channel in 0..3 {
                                average[channel] +=
                                    f32::from(pixels[offset + channel]) / 127.5 - 1.0;
                            }
                        }
                        let inverse_length =
                            (average.iter().map(|value| value * value).sum::<f32>()).sqrt().recip();
                        next.extend(
                            average
                                .map(|value| linear_unorm_byte(value * inverse_length * 0.5 + 0.5)),
                        );
                    }
                    TextureMipKind::SrgbColor | TextureMipKind::LinearData => {
                        for channel in 0..3 {
                            let mut average = 0.0;
                            for offset in offsets {
                                let value = f32::from(pixels[offset + channel]) / 255.0;
                                average += match kind {
                                    TextureMipKind::SrgbColor => srgb_to_linear(value),
                                    TextureMipKind::LinearData => value,
                                    TextureMipKind::Normal => unreachable!(),
                                };
                            }
                            average *= 0.25;
                            let encoded = match kind {
                                TextureMipKind::SrgbColor => encode_srgb_byte(average),
                                TextureMipKind::LinearData => linear_unorm_byte(average),
                                TextureMipKind::Normal => unreachable!(),
                            };
                            next.push(encoded);
                        }
                    }
                }
                let alpha =
                    offsets.iter().map(|offset| u16::from(pixels[*offset + 3])).sum::<u16>() / 4;
                next.push(u8::try_from(alpha).unwrap_or(255));
            }
        }
        levels.push(next.clone());
        pixels = next;
        width = next_width;
    }
    levels
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
}

#[allow(clippy::cast_precision_loss)] // These indices are bounded to 0..64 and exactly representable as f32.
fn texture_coordinate(index: u32) -> f32 {
    index as f32 / TEXTURE_SIZE as f32
}

fn encode_srgb_byte(value_linear: f32) -> u8 {
    let srgb = if value_linear <= 0.003_130_8 {
        value_linear * 12.92
    } else {
        1.055 * value_linear.powf(1.0 / 2.4) - 0.055
    };
    linear_unorm_byte(srgb)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Clamped and rounded into the exact u8 range.
fn linear_unorm_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::{PbrMaterialDesc, TextureMipKind, encode_srgb_byte, generate_mips, srgb_to_linear};

    #[test]
    fn sdr_color_conversion_obeys_srgb_transfer_function() {
        assert!((srgb_to_linear(0.5) - 0.214_041_14).abs() < 1.0e-6);
        assert!((f32::from(encode_srgb_byte(0.214_041_14)) / 255.0 - 0.502).abs() < 0.003);
    }

    #[test]
    fn texture_mips_reach_one_texel_and_preserve_alpha() {
        let base = vec![128_u8; (64 * 64 * 4) as usize];
        let levels = generate_mips(base, TextureMipKind::LinearData);
        assert_eq!(levels.len(), 7);
        assert_eq!(levels.last().map(Vec::len), Some(4));
        assert_eq!(levels.last().map(|level| level[3]), Some(128));
    }

    #[test]
    fn material_rejects_invalid_pbr_factors() {
        assert!(PbrMaterialDesc::new([0.5, 0.5, 0.5], 0.5, 0.5, 1.0, 1.0).is_ok());
        assert!(PbrMaterialDesc::new([0.5, 0.5, f32::NAN], 0.5, 0.5, 1.0, 1.0).is_err());
        assert!(PbrMaterialDesc::new([0.5, 0.5, 0.5], 1.1, 0.5, 1.0, 1.0).is_err());
    }
}
