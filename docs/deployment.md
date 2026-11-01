# Windows Release Deployment

The game release is distributed as a single `nord.exe`. Runtime shaders are
compiled during the developer build and embedded into that executable; players
do not need the shader source folder, DXC, a configuration file, or the Visual
C++ runtime redistributable. The MSVC C runtime is linked statically for the
Windows release target in `.cargo/config.toml`.

The executable still relies on Windows system components and a working graphics
driver. The current renderer uses Direct3D 12, so the machine must provide a
supported Windows version, a D3D12-capable GPU, and its driver. Those are system
requirements, not files to copy beside the game.

At startup, Nord uses built-in configuration defaults when no optional config
file is present. Logs and crash reports are created under the user's local
application data directory when needed; they are generated runtime output and
are not prerequisites for launching the game.

## Release check

`scripts/verify_single_file_release.ps1` builds the optimized game, copies only
`nord.exe` into an isolated directory, and launches it there with no config file
and an invalid DXC override. It verifies that the process stays alive and that
the isolated directory still contains only the executable. Run it on Windows
with a D3D12-capable GPU and driver.
