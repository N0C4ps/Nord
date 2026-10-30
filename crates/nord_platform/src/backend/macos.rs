use std::env;
use std::path::PathBuf;

fn home_dir() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_else(env::temp_dir)
}

pub(crate) fn game_paths(application_id: &str) -> (PathBuf, PathBuf, PathBuf) {
    let home = home_dir();
    (
        home.join("Library/Application Support").join(application_id),
        home.join("Library/Preferences").join(application_id),
        home.join("Library/Caches").join(application_id),
    )
}
