//! Game-owned material descriptions and their GPU/shader realization.

use nord_app::{
    ApplicationBufferDesc, ApplicationBufferMemory, ApplicationBufferUsage,
    ApplicationCompiledShader, ApplicationDiagnosticError, ApplicationErrorContext,
    ApplicationGraphicsPipelineDesc, ApplicationPipelineDescriptorBinding, ApplicationRenderer,
    ApplicationShaderBindingKind, ApplicationShaderCompileOptions, ApplicationShaderCompiler,
    ApplicationShaderStage, ApplicationTextureFormat, ApplicationVertexAttribute,
    ApplicationVertexFormat,
};
use nord_app::{ApplicationDescriptorHandle, ApplicationGpuResourceHandle};
use std::path::PathBuf;

const MATERIAL_CONSTANT_BYTES: u64 = 256;
const MATERIAL_CONSTANT_REGISTER: u32 = 1;
const MATERIAL_SHADER: &str = "nord_basic_forward.hlsl";

/// Small game material contract for the first opaque forward pass.
///
/// Colors are linear-light RGB values in the normalized range. Opaque
/// materials always write alpha 1 and do not enable blending.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BasicOpaqueMaterialDesc {
    base_color_linear: [f32; 3],
}

impl BasicOpaqueMaterialDesc {
    /// Create an opaque material with a linear-light base color.
    ///
    /// # Errors
    /// Returns an error if any component is non-finite or outside `0..=1`.
    pub(super) fn new(base_color_linear: [f32; 3]) -> nord_app::ApplicationResult<Self> {
        if base_color_linear
            .iter()
            .any(|component| !component.is_finite() || !(0.0..=1.0).contains(component))
        {
            return Err(ApplicationDiagnosticError::msg(
                "opaque material base color must contain finite linear RGB values in 0..=1",
            ));
        }
        Ok(Self { base_color_linear })
    }
}

/// One material's immutable shaders/pipeline and game-owned constant binding.
pub(super) struct BasicOpaqueMaterial {
    pipeline: ApplicationGraphicsPipelineDesc,
    vertex_shader: ApplicationCompiledShader,
    pixel_shader: ApplicationCompiledShader,
    constants: ApplicationGpuResourceHandle,
    constants_view: ApplicationDescriptorHandle,
    binding: ApplicationPipelineDescriptorBinding,
}

impl BasicOpaqueMaterial {
    /// Load/compile the game's forward shaders and create an opaque depth-tested material.
    ///
    /// # Errors
    /// Returns diagnostics from shader compilation, reflection validation, GPU allocation,
    /// descriptor creation, or native pipeline creation.
    pub(super) fn load(
        renderer: &mut ApplicationRenderer,
        descriptor: BasicOpaqueMaterialDesc,
    ) -> nord_app::ApplicationResult<Self> {
        let (vertex_shader, pixel_shader) = load_shader_program()?;
        require_binding(&vertex_shader, ApplicationShaderStage::Vertex, "CameraData", 0)?;
        require_binding(
            &pixel_shader,
            ApplicationShaderStage::Pixel,
            "MaterialData",
            MATERIAL_CONSTANT_REGISTER,
        )?;

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
                semantic: "NORMAL".into(),
                semantic_index: 0,
                format: ApplicationVertexFormat::Float3,
                input_slot: 0,
                byte_offset: 12,
                per_instance: false,
                instance_step_rate: 0,
            });
        renderer
            .create_graphics_pipeline(&pipeline, &vertex_shader, &pixel_shader, 1)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's opaque forward pipeline")?;

        let constants = renderer
            .create_buffer(
                ApplicationBufferDesc::new(
                    MATERIAL_CONSTANT_BYTES,
                    ApplicationBufferUsage::CONSTANT,
                )?
                .with_memory(ApplicationBufferMemory::Upload),
            )
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating the game's material constants")?;
        let constants_view = renderer
            .create_constant_buffer_view(constants)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's material constant binding")?;
        let binding = ApplicationPipelineDescriptorBinding {
            kind: ApplicationShaderBindingKind::ConstantBuffer,
            register: MATERIAL_CONSTANT_REGISTER,
            space: 0,
            descriptor: constants_view,
        };
        renderer
            .write_buffer(constants, 0, &encode_material_constants(descriptor))
            .map_err(ApplicationDiagnosticError::msg)
            .context("writing the game's opaque material parameters")?;

        Ok(Self { pipeline, vertex_shader, pixel_shader, constants, constants_view, binding })
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
    pub(super) const fn binding(&self) -> ApplicationPipelineDescriptorBinding {
        self.binding
    }

    #[must_use]
    pub(super) const fn constants_resource(&self) -> ApplicationGpuResourceHandle {
        self.constants
    }

    /// Release the material-owned descriptor and upload resource after queue idle.
    ///
    /// # Errors
    /// Returns a renderer error if either GPU object cannot be retired.
    pub(super) fn shutdown(
        &self,
        renderer: &mut ApplicationRenderer,
    ) -> nord_app::ApplicationResult<()> {
        renderer
            .destroy_descriptor(self.constants_view)
            .map_err(ApplicationDiagnosticError::msg)
            .context("retiring the game's material constant descriptor")?;
        renderer
            .destroy_resource(self.constants)
            .map_err(ApplicationDiagnosticError::msg)
            .context("retiring the game's material constant buffer")
    }
}

fn load_shader_program()
-> nord_app::ApplicationResult<(ApplicationCompiledShader, ApplicationCompiledShader)> {
    let shader_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("shaders")
        .join(MATERIAL_SHADER);
    let compiler = ApplicationShaderCompiler::default();
    let vertex = compiler
        .compile(&ApplicationShaderCompileOptions::new(
            &shader_path,
            ApplicationShaderStage::Vertex,
            "VSMain",
        ))
        .map_err(ApplicationDiagnosticError::msg)
        .context("loading and compiling the game's forward vertex shader")?;
    let pixel = compiler
        .compile(&ApplicationShaderCompileOptions::new(
            &shader_path,
            ApplicationShaderStage::Pixel,
            "PSMain",
        ))
        .map_err(ApplicationDiagnosticError::msg)
        .context("loading and compiling the game's forward pixel shader")?;
    for (stage, shader) in [("vertex", &vertex), ("pixel", &pixel)] {
        for diagnostic in &shader.diagnostics {
            nord_app::warn!(
                target: "nord::render::shader",
                shader_stage = stage,
                diagnostic = %diagnostic.message,
                "Shader compiler emitted a diagnostic"
            );
        }
    }
    Ok((vertex, pixel))
}

fn require_binding(
    shader: &ApplicationCompiledShader,
    stage: ApplicationShaderStage,
    name: &str,
    register: u32,
) -> nord_app::ApplicationResult<()> {
    let matches = shader.stage == stage
        && shader.reflection.bindings().iter().any(|binding| {
            binding.name == name
                && binding.kind == ApplicationShaderBindingKind::ConstantBuffer
                && binding.register == register
                && binding.space == 0
                && binding.count == 1
        });
    if matches {
        Ok(())
    } else {
        Err(ApplicationDiagnosticError::msg(format!(
            "shader contract mismatch: {name} must be one constant buffer at b{register}, space0, in {stage:?} stage"
        )))
    }
}

fn encode_material_constants(descriptor: BasicOpaqueMaterialDesc) -> [u8; 256] {
    let mut bytes = [0_u8; 256];
    for (index, component) in descriptor.base_color_linear.into_iter().chain([1.0]).enumerate() {
        let offset = index * std::mem::size_of::<f32>();
        bytes[offset..offset + 4].copy_from_slice(&component.to_le_bytes());
    }
    bytes
}
