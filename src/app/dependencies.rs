use nord_app::{ApplicationServicesBuilder, ApplicationServicesError};
use std::sync::Arc;

pub(super) struct AppDependencies {
    pub(super) settings: Arc<GameSettings>,
    pub(super) logger: Arc<GameLogger>,
}

impl AppDependencies {
    pub(super) fn build() -> Result<Self, ApplicationServicesError> {
        let mut builder = ApplicationServicesBuilder::new();
        builder.register(GameSettings { title: "Nord starting" })?;
        builder.register(GameLogger { channel: "GAME" })?;

        // Keep the DI implementation behind this game-owned composition adapter.
        let container = builder.build();
        Ok(Self {
            settings: container.get::<GameSettings>()?,
            logger: container.get::<GameLogger>()?,
        })
    }
}

pub(super) struct GameSettings {
    pub(super) title: &'static str,
}

pub(super) struct GameLogger {
    channel: &'static str,
}

impl GameLogger {
    pub(super) fn info(&self, message: &str) {
        nord_log::info(&format!("[{}] {message}", self.channel));
    }
}
