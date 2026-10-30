# Nord

Nord is a voxel sandbox game built on Nord Tech.

## Repository layout

- `crates/`: reusable engine and infrastructure crates (`nord_*`), with no Nord-specific gameplay rules.
- `src/`: the game package, including game-specific use and configuration of engine systems.
- `scripts/`: PowerShell build and run entry points.
- `target/`: Cargo build products, including debug and release executables.

## Architecture

See [docs/architecture.md](docs/architecture.md) for the rules that keep Nord Tech crates isolated and let the game compose them through their public APIs.

The typed DI registry lives in `crates/nord_dependency`; its intended use and boundaries are described in [docs/dependency-injection.md](docs/dependency-injection.md).

## Toolchain

The repository pins Rust `1.97.1` in `rust-toolchain.toml` and requests Clippy, rustfmt, and rust-analyzer. Install Rust through rustup, then use `cargo fmt --all`, `cargo lint`, and `cargo check --workspace`.

## Build and run

```powershell
./scripts/build_debug.ps1
./scripts/build_fast.ps1
./scripts/build_release.ps1
./scripts/run_debug.ps1
./scripts/run_fast.ps1
./scripts/run_release.ps1
./scripts/validate.ps1
```

`release-fast` is an optimized build for quicker iteration; `release` is the shipping profile. Cargo writes build artifacts under `target/debug/`, `target/release-fast/`, and `target/release/`.

For the full build pipeline and each script's purpose, see [docs/build-system.md](docs/build-system.md).
