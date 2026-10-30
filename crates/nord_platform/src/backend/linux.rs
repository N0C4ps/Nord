use std::env;
use std::path::PathBuf;

fn home_dir() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_else(env::temp_dir)
}

fn xdg_dir(variable: &str, fallback: PathBuf) -> PathBuf {
    env::var_os(variable).map(PathBuf::from).filter(|path| path.is_absolute()).unwrap_or(fallback)
}

pub(crate) fn game_paths(application_id: &str) -> (PathBuf, PathBuf, PathBuf) {
    let home = home_dir();
    let data = xdg_dir("XDG_DATA_HOME", home.join(".local/share"));
    let config = xdg_dir("XDG_CONFIG_HOME", home.join(".config"));
    let cache = xdg_dir("XDG_CACHE_HOME", home.join(".cache"));
    (data.join(application_id), config.join(application_id), cache.join(application_id))
}
