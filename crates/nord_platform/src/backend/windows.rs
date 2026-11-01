use std::env;
use std::path::PathBuf;

pub(crate) mod display;
pub(crate) mod window;

pub(crate) fn game_paths(application_id: &str) -> (PathBuf, PathBuf, PathBuf) {
    let local = env::var_os("LOCALAPPDATA").map_or_else(env::temp_dir, PathBuf::from);
    let roaming = env::var_os("APPDATA").map_or_else(|| local.clone(), PathBuf::from);
    (
        local.join(application_id),
        roaming.join(application_id),
        local.join(application_id).join("Cache"),
    )
}
