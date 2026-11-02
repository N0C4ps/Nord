#![cfg_attr(all(target_os = "windows", not(test)), windows_subsystem = "windows")]

mod app;
mod camera;
mod gameloop;
mod physics;
mod render;
mod world;

use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(error) = app::run() {
        if !error.was_logged() {
            eprintln!("Nord could not initialize: {error:#}");
        }
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
