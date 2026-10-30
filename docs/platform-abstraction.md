# Nord Platform Abstraction Layer

## Ownership and dependency direction

`crates/nord_platform` implements reusable native platform capabilities. It
contains no Nord gameplay rules and does not create windows. `crates/nord_app`
owns the application-facing facade (`ApplicationPlatform`) and is the only
crate that depends on `nord_platform`. The game uses that facade through its
application service registry; game modules must not import `nord_platform`.

This boundary lets the platform implementation evolve independently while
keeping the game's integration contract in `nord_app`. Source compatibility
depends on keeping that public contract compatible.

## Capabilities

- **Backend identity:** native backend and target architecture are selected at
  compile time. Windows is the primary development target; Linux and macOS
  backend modules are selected through target-specific compilation.
- **Application paths:** data, config, cache, and temporary paths are resolved
  without creating directories implicitly. Call `ensure_*_dir` at the point
  where the corresponding resource is needed.
- **Monotonic time:** elapsed timestamps use `std::time::Instant`, not wall
  clock time, so system clock adjustments do not move frame or event time
  backwards.
- **Explicit file resources:** opening returns an opaque, generation-checked
  handle; reading, writing, seeking, flushing, and closing go through the
  owning filesystem capability. Stale/closed handles are rejected. The
  synchronous API is for control/configuration and modest I/O; streamed asset
  workloads should use a future asynchronous asset subsystem.
- **Normalized event queue:** lifecycle, window, keyboard, and pointer events
  share a platform-neutral payload, monotonic timestamp, and ordered sequence.
  Producers enqueue into a bounded queue; a full queue returns an explicit
  error, and the application drains into a reusable buffer. Window/input event
  types are contracts for future native subsystems; this crate does not yet
  create a window or poll OS input.
- **Opaque window identity:** a window handle does not expose its numeric value
  to game code. A future window owner will allocate and interpret IDs.

## Native backends

The backend is selected with Rust's `cfg(target_os)` at compile time. Native
details stay in `crates/nord_platform/src/backend/`; capability APIs remain
platform-neutral. Windows resolves user data using `LOCALAPPDATA` and roaming
configuration using `APPDATA`. Linux follows XDG data/config/cache environment
variables and standard home-directory fallbacks. macOS uses the user Library
locations for Application Support, Preferences, and Caches.

Use platform-native `PathBuf` paths and `OsStr` values instead of assuming that
native paths are UTF-8. Keep raw OS handles private to the backend. Future
OS-specific capabilities should use narrow, owned APIs with backend-specific
implementations selected behind the same facade.

## Threading and lifecycle

`ApplicationPlatform` is owned by the application service container. The event
emitter is cloneable for subsystem producers. Queue synchronization protects
sequence assignment and insertion together, so sequence numbers represent
enqueue order when producers contend. The queue is bounded to prevent
unbounded growth; producers must handle backpressure rather than silently
dropping events.

File operations are synchronized and resource lifetime is explicit. Close file
handles when their owner is done. The platform object itself does not open
files, create directories, start threads, or initialize a GUI event loop as a
side effect of construction.

## Scope and extension points

This initial PAL establishes backend selection, application directories,
monotonic time, explicit file resources, and normalized events. It does not
claim to implement every future platform service. Window creation, raw input
polling, native dialogs, clipboard, controllers, display enumeration, power
notifications, and asynchronous I/O should be introduced as separate
capabilities when needed. A future `nord_window` should own window creation
and native GUI event-loop constraints; it should send normalized window/input
events through the PAL contract.

## Validation

The workspace architecture check enforces that `src/` does not import
`nord_platform` and that `nord_app` is its sole direct engine-crate consumer.
The workspace build checks the currently selected native backend. Linux and
macOS modules should also be compiled in CI runners for those operating systems
before shipping support on those targets.

## Technical references

- [Rust conditional compilation reference](https://doc.rust-lang.org/reference/conditional-compilation.html)
  for compile-time backend selection.
- [`std::time::Instant`](https://doc.rust-lang.org/stable/std/time/struct.Instant.html)
  for monotonic time and the platform-native clock behavior.
- [`std::path`](https://doc.rust-lang.org/stable/std/path/) for preserving
  native path representations.
- [winit event-loop documentation](https://docs.rs/winit/latest/src/winit/event_loop.rs.html)
  for the main-thread and once-per-application constraints to account for when
  the separate window subsystem is designed.
