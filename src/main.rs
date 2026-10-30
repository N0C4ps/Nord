mod app;
mod gameloop;
mod physics;
mod render;
mod world;

fn main() {
    if let Err(error) = app::run() {
        eprintln!("Nord failed to start: {error}");
        std::process::exit(1);
    }
}
