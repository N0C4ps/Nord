#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub(crate) use linux::game_paths;
#[cfg(target_os = "macos")]
pub(crate) use macos::game_paths;
#[cfg(target_os = "windows")]
pub(crate) use windows::display::WindowsDisplayBackend;
#[cfg(target_os = "windows")]
pub(crate) use windows::game_paths;
#[cfg(target_os = "windows")]
pub(crate) use windows::window::WindowsWindowBackend;

#[cfg(target_os = "windows")]
const BACKEND: super::BackendKind = super::BackendKind::Windows;
#[cfg(target_os = "linux")]
const BACKEND: super::BackendKind = super::BackendKind::Linux;
#[cfg(target_os = "macos")]
const BACKEND: super::BackendKind = super::BackendKind::MacOs;

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
compile_error!("nord_platform currently supports Windows, Linux, and macOS targets");

pub(super) fn platform_info() -> super::PlatformInfo {
    super::PlatformInfo { backend: BACKEND, architecture: std::env::consts::ARCH }
}
