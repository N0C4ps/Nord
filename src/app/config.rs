use serde::{Deserialize, Serialize};

/// Game-owned configuration schema for composing engine capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct GameConfig {
    pub(crate) application: ApplicationConfig,
    pub(crate) window: WindowConfig,
    pub(crate) threading: ThreadingConfig,
    pub(crate) tasks: TasksConfig,
    pub(crate) timing: TimingConfig,
}

impl GameConfig {
    pub(super) fn validate(&self) -> Result<(), String> {
        let title = self.application.title.trim();
        if title.is_empty() {
            return Err("application.title must not be empty".to_owned());
        }
        if title.len() > 128 {
            return Err("application.title must be at most 128 bytes".to_owned());
        }
        if self.window.width == 0 || self.window.height == 0 {
            return Err("window.width and window.height must be greater than zero".to_owned());
        }
        if !(1..=60_000).contains(&self.timing.max_frame_delta_ms) {
            return Err("timing.max_frame_delta_ms must be between 1 and 60000".to_owned());
        }
        if !(1..=10_000_000_000).contains(&self.timing.fixed_step_ns) {
            return Err("timing.fixed_step_ns must be between 1 and 10000000000".to_owned());
        }
        if !(1..=1_000).contains(&self.timing.max_steps_per_frame) {
            return Err("timing.max_steps_per_frame must be between 1 and 1000".to_owned());
        }
        if !self.timing.time_scale.is_finite() || !(0.0..=100.0).contains(&self.timing.time_scale) {
            return Err("timing.time_scale must be finite and between 0 and 100".to_owned());
        }
        if self.threading.worker_threads == Some(0) {
            return Err("threading.worker_threads must be greater than zero".to_owned());
        }
        if self.tasks.queue_capacity == 0 {
            return Err("tasks.queue_capacity must be greater than zero".to_owned());
        }
        if self.tasks.max_in_flight == Some(0) {
            return Err("tasks.max_in_flight must be greater than zero when set".to_owned());
        }
        if self.tasks.priority_aging_interval_ms == 0 {
            return Err("tasks.priority_aging_interval_ms must be greater than zero".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct WindowConfig {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) resizable: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self { width: 1280, height: 720, resizable: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ApplicationConfig {
    pub(crate) title: String,
}

impl Default for ApplicationConfig {
    fn default() -> Self {
        Self { title: "Nord".to_owned() }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ThreadingConfig {
    /// `None` selects a worker count from available parallelism.
    pub(crate) worker_threads: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct TasksConfig {
    pub(crate) queue_capacity: usize,
    pub(crate) max_in_flight: Option<usize>,
    pub(crate) priority_aging_interval_ms: u64,
}

impl Default for TasksConfig {
    fn default() -> Self {
        Self { queue_capacity: 2_048, max_in_flight: None, priority_aging_interval_ms: 250 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct TimingConfig {
    pub(crate) max_frame_delta_ms: u64,
    pub(crate) fixed_step_ns: u64,
    pub(crate) max_steps_per_frame: u32,
    pub(crate) time_scale: f64,
}

impl Default for TimingConfig {
    fn default() -> Self {
        Self {
            max_frame_delta_ms: 250,
            fixed_step_ns: 16_666_667,
            max_steps_per_frame: 8,
            time_scale: 1.0,
        }
    }
}
