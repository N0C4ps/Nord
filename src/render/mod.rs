use nord_app::{
    ApplicationClearColor, ApplicationDiagnosticError, ApplicationErrorContext,
    ApplicationPlatform, ApplicationRenderConfig, ApplicationRenderer, ApplicationResult,
    ApplicationWindowHandle,
};

/// Game-owned policy that connects the generic renderer to Nord's app loop.
pub(super) struct GameRenderer {
    renderer: Option<ApplicationRenderer>,
}

impl GameRenderer {
    pub(super) fn initialize(
        platform: &ApplicationPlatform,
        window: ApplicationWindowHandle,
        width: u32,
        height: u32,
    ) -> ApplicationResult<Self> {
        let mut renderer = ApplicationRenderer::initialize(
            platform,
            window,
            ApplicationRenderConfig::new(width, height),
        )
        .map_err(ApplicationDiagnosticError::msg)
        .context("initializing Nord's graphics infrastructure")?;
        let allocator_stats = renderer.gpu_allocator_stats();
        nord_app::debug!(
            target: "nord::render",
            reserved_gpu_bytes = allocator_stats.reserved_bytes,
            "Renderer resource bookkeeping services are ready"
        );
        nord_app::info!(
            name: "renderer_initialized",
            target: "nord::render",
            backend = ?renderer.backend(),
            "Graphics device and presentation infrastructure initialized"
        );
        Ok(Self { renderer: Some(renderer) })
    }

    pub(super) fn render_frame(&mut self) -> ApplicationResult<()> {
        self.renderer
            .as_mut()
            .ok_or_else(|| ApplicationDiagnosticError::msg("renderer has already shut down"))?
            .render_frame(ApplicationClearColor::rgba(0.025, 0.035, 0.055, 1.0))
            .map_err(ApplicationDiagnosticError::msg)
            .context("recording and presenting the Nord frame")
    }

    pub(super) fn wait_for_frame(&self) -> ApplicationResult<()> {
        self.renderer
            .as_ref()
            .ok_or_else(|| ApplicationDiagnosticError::msg("renderer has already shut down"))?
            .wait_for_frame()
            .map_err(ApplicationDiagnosticError::msg)
            .context("waiting for graphics frame readiness")
    }

    pub(super) fn resize(&mut self, width: u32, height: u32) -> ApplicationResult<()> {
        self.renderer
            .as_mut()
            .ok_or_else(|| ApplicationDiagnosticError::msg("renderer has already shut down"))?
            .resize(width, height)
            .map_err(ApplicationDiagnosticError::msg)
            .context("resizing the Nord render surface")
    }

    pub(super) fn surface_suspended(&self) -> bool {
        self.renderer.as_ref().is_none_or(ApplicationRenderer::surface_suspended)
    }

    pub(super) fn shutdown(&mut self) -> ApplicationResult<()> {
        let Some(mut renderer) = self.renderer.take() else {
            return Ok(());
        };
        let result = renderer
            .wait_idle()
            .map_err(ApplicationDiagnosticError::msg)
            .context("waiting for graphics work before window shutdown");
        drop(renderer);
        result
    }
}
