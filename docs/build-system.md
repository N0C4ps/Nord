# Build system

## Build model

Nord uses one Cargo workspace. The root package is the game executable (`nord`), and `crates/nord_*` are independent library packages. Cargo resolves and compiles the dependency graph, then links the game executable; scripts do not manually concatenate Rust source files or crate outputs. This keeps dependency ordering, incremental rebuilds, and linking under Cargo.

The root game package is the workspace's default member so a plain Cargo command from the root targets the game. Scripts that are meant to compile or inspect the complete project explicitly pass `--workspace`. The game package declares its local Nord Tech libraries as path dependencies, so building/running the game also compiles the libraries it consumes.

All scripts locate the repository relative to their own file, so they work regardless of the caller's current directory. They call Cargo with `--locked` for build/validation operations, preserve the Cargo exit code, and use the shared `scripts/_common.ps1` helper. Cargo's target directory is configured as `target/` in `.cargo/config.toml`.

## Profiles

- `dev` / `build_debug.ps1`: fast compilation and full debug information, at `target/debug/`.
- `release-fast` / `build_fast.ps1`: optimized code with LTO disabled and more codegen units, intended for quicker performance iteration, at `target/release-fast/`.
- `release` / `build_release.ps1`: optimized shipping build with thin LTO and one codegen unit, at `target/release/`.

The release profile favors optimization quality and may compile/link more slowly. The fast release profile exists for repeated profiling or playtesting where a full shipping build is unnecessary.

## Script catalog

| Script | Purpose |
| --- | --- |
| `build_debug.ps1` | Build every workspace package in debug mode. |
| `build_fast.ps1` | Build every workspace package with `release-fast`. |
| `build_release.ps1` | Build every workspace package with the shipping `release` profile. |
| `run_debug.ps1` | Build and run the Nord game in debug mode; forwards remaining arguments to the game. |
| `run_fast.ps1` | Build and run the game with `release-fast`. |
| `run_release.ps1` | Build and run the game with the shipping profile. |
| `check.ps1` | Type-check all workspace packages, targets, and features without full code generation. |
| `lint.ps1` | Run Clippy across the workspace and fail on warnings. |
| `format.ps1` | Format all workspace Rust code. |
| `format_check.ps1` | Check formatting without modifying files. |
| `docs.ps1` | Generate workspace documentation under `target/doc-build/doc/`. |
| `validate.ps1` | Run format check, Clippy, and workspace check in sequence. |
| `build_timed.ps1` | Build with Cargo timing reports; accepts `-Release` or `-Fast`. |
| `clean.ps1` | Remove generated Cargo outputs from `target/`. |

Examples:

```powershell
./scripts/run_debug.ps1 --help
./scripts/build_timed.ps1 -Release
./scripts/validate.ps1
```

Cargo writes timing reports to `target/cargo-timings/`. Keep generated files in `target/`; do not copy executables or intermediate compiler output into source folders.

## Adding build steps

Prefer native Cargo capabilities over custom build orchestration. Add a crate as a workspace member and path dependency where needed. Use a crate `build.rs` only when that package genuinely needs native compilation, generated source, or target-specific setup; keep generated/intermediate outputs inside Cargo's `OUT_DIR`. Keep validation commands reproducible and lockfile-aware.

## Research references

- [Cargo Workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html): package selection, shared lockfile, profiles, and shared target directory.
- [Cargo Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html): development/release settings, LTO, and codegen-unit tradeoffs.
- [Cargo Build Cache](https://doc.rust-lang.org/cargo/reference/build-cache.html): artifact layout under `target/`.
- [Cargo Timing Reports](https://doc.rust-lang.org/cargo/reference/timings.html): diagnosing compilation bottlenecks.
- [Cargo Continuous Integration](https://doc.rust-lang.org/cargo/guide/continuous-integration.html): formatting, Clippy, workspace checks, and warning policies.
- [Cargo Build Scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html): when build scripts are appropriate and where they should write outputs.
