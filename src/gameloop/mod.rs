use crate::app::config::GameConfig;
use nord_app::{
    ApplicationClockConfig, ApplicationErrorContext, ApplicationFixedStepAccumulator,
    ApplicationFixedStepConfig, ApplicationFpsCounter, ApplicationFpsSnapshot,
    ApplicationFrameClock, ApplicationFrameTime,
};
use std::time::Duration;

/// Game-owned per-iteration work coordinated by the application runtime.
pub(super) struct GameLoop {
    iterations: u64,
    scheduled_steps: u64,
    _frame_context_reservation: nord_app::ApplicationMemoryReservation,
    frame_scratch: nord_app::ApplicationScratchArena,
    clock: ApplicationFrameClock,
    fixed_steps: ApplicationFixedStepAccumulator,
    fixed_step: Duration,
    fps: ApplicationFpsCounter,
}

impl GameLoop {
    pub(super) fn new(
        memory: &nord_app::ApplicationMemoryManager,
        config: &GameConfig,
    ) -> nord_app::ApplicationResult<Self> {
        let mut clock = ApplicationFrameClock::new(ApplicationClockConfig::new(
            Duration::from_millis(config.timing.max_frame_delta_ms),
        ));
        clock
            .set_time_scale(config.timing.time_scale)
            .map_err(nord_app::ApplicationDiagnosticError::msg)
            .context("applying configured game time scale")?;
        let fixed_step_config = ApplicationFixedStepConfig::new(
            Duration::from_nanos(config.timing.fixed_step_ns),
            config.timing.max_steps_per_frame,
        )
        .map_err(nord_app::ApplicationDiagnosticError::msg)
        .context("building configured fixed-step simulation")?;
        let fixed_step = fixed_step_config.step();
        let frame_context_bytes = u64::try_from(std::mem::size_of::<FrameContext>())?;
        let frame_context_reservation = memory
            .reserve(frame_context_bytes)
            .map_err(nord_app::ApplicationDiagnosticError::msg)
            .context("reserving memory for the game frame context")?;
        Ok(Self {
            iterations: 0,
            scheduled_steps: 0,
            _frame_context_reservation: frame_context_reservation,
            frame_scratch: memory.create_scratch_arena(None),
            clock,
            fixed_steps: ApplicationFixedStepAccumulator::new(fixed_step_config),
            fixed_step,
            fps: ApplicationFpsCounter::default(),
        })
    }

    pub(super) fn advance<FixedUpdate>(
        &mut self,
        platform_event_count: usize,
        mut fixed_update: FixedUpdate,
    ) -> nord_app::ApplicationResult<()>
    where
        FixedUpdate: FnMut(Duration) -> nord_app::ApplicationResult<()>,
    {
        nord_app::ensure!(self.iterations < u64::MAX, "game loop iteration counter exhausted");
        let next_iteration = self.iterations + 1;
        self.frame_scratch.reset();
        let time = self.clock.tick();
        let fixed_plan = self.fixed_steps.advance(time.delta);
        let _fps_sample = self.fps.record_frame(time.raw_delta);
        for _ in 0..fixed_plan.steps {
            fixed_update(self.fixed_step).context("running a fixed-step game system")?;
        }

        {
            let frame = self
                .frame_scratch
                .try_alloc(FrameContext {
                    time,
                    platform_event_count,
                    fixed_steps: fixed_plan.steps,
                    interpolation_alpha: fixed_plan.interpolation_alpha,
                })
                .map_err(nord_app::ApplicationDiagnosticError::msg)
                .context("allocating the current frame context")?;
            nord_app::ensure_eq!(
                frame.time.frame_index,
                next_iteration,
                "frame scratch context must match the next game iteration"
            );
            nord_app::ensure_eq!(
                frame.platform_event_count,
                platform_event_count,
                "frame scratch context must preserve the polled event count"
            );
            nord_app::ensure!(
                frame.interpolation_alpha >= 0.0 && frame.interpolation_alpha < 1.0,
                "fixed-step interpolation alpha must stay within [0, 1)"
            );
            self.iterations = frame.time.frame_index;
            self.scheduled_steps =
                self.scheduled_steps.saturating_add(u64::from(frame.fixed_steps));
        }

        self.frame_scratch.reset();
        Ok(())
    }

    pub(super) const fn iterations(&self) -> u64 {
        self.iterations
    }

    pub(super) const fn scheduled_steps(&self) -> u64 {
        self.scheduled_steps
    }

    pub(super) const fn dropped_simulation_time(&self) -> Duration {
        self.fixed_steps.total_dropped()
    }

    pub(super) const fn latest_fps(&self) -> Option<ApplicationFpsSnapshot> {
        self.fps.latest()
    }
}

struct FrameContext {
    time: ApplicationFrameTime,
    platform_event_count: usize,
    fixed_steps: u32,
    interpolation_alpha: f64,
}
