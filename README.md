# Nord

Nord is a voxel sandbox game built on Nord Tech.

## Repository layout

- `crates/`: reusable engine and infrastructure crates (`nord_*`), with no Nord-specific gameplay rules.
- `src/`: the game package, including game-specific use and configuration of engine systems.
- `scripts/`: PowerShell build and run entry points.
- `target/`: Cargo build products, including debug and release executables.

## Toolchain

The repository pins the Rust channel to `stable` in `rust-toolchain.toml` and requests Clippy, rustfmt, and rust-analyzer. Install Rust through rustup, then use `cargo fmt --all`, `cargo lint`, and `cargo check --workspace`.

## Build and run

```powershell
./scripts/build_debug.ps1
./scripts/build_release.ps1
./scripts/run_debug.ps1
./scripts/run_release.ps1
```

Cargo writes build artifacts under `target/debug/` and `target/release/`.
