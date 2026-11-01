# Nord Platform Abstraction Layer

## Ownership and dependency direction

`crates/nord_platform` implements reusable native platform capabilities. It
contains no Nord gameplay rules. `crates/nord_app` owns the application-facing
facade (`ApplicationPlatform`) and is the only crate that depends on
`nord_platform`. Game modules use that facade through the application service
registry; game code must not import `nord_platform` or native OS APIs.

This boundary lets the platform implementation evolve independently while
keeping the game's integration contract in `nord_app`. Source compatibility
depends on keeping that public contract compatible.

## Capabilities

- **Backend identity:** native backend and target architecture are selected at
  compile time. Windows is the primary development target.
- **Application paths:** data, config, cache, and temporary paths are resolved
  without creating directories implicitly. Call `ensure_*_dir` when needed.
- **Monotonic time:** elapsed timestamps use `std::time::Instant`, so wall
  clock adjustments do not move frame or event time backwards.
- **Explicit file resources:** opening returns an opaque handle; reading,
  writing, seeking, flushing, and closing go through the owning filesystem.
- **Native window capability:** `WindowSystem` creates and owns top-level
  windows using opaque `WindowHandle`s. Window configuration, close, resize,
  focus, and DPI events are platform-neutral; raw HWND values stay inside the
  Windows backend. Window creation, message pumping, and destruction are bound
  to the thread that initialized the platform. Wrong-thread calls return an
  explicit error.
- **Display queries:** `DisplayManager` returns fresh snapshots of connected
  monitors: device name, virtual-desktop bounds, usable work area, and primary
  status. It can find the monitor containing a point. Startup logs a compact
  primary-display summary. Snapshots avoid retaining stale native monitor
  handles across display hot-plug changes.
- **Normalized event queue:** events have monotonic timestamps and an ordered
  sequence. Producers enqueue into a bounded queue and the application drains
  into a reusable buffer. Queue saturation returns an explicit error. Current
  Win32 messages produce close, resize, focus, and DPI notifications. Keyboard
  and pointer messages are normalized by the platform backend into the separate
  `nord_inputs` contracts.

## Native backends

Backend selection uses Rust `cfg(target_os)` and native details stay in
`crates/nord_platform/src/backend/`. Windows uses a thin `windows-sys` binding
layer. Its window procedure does only small event translations and delegates
unhandled messages to `DefWindowProcW`; the application pumps pending messages
without blocking in its frame loop. The game executable embeds an application
manifest that requests Per-Monitor-V2 DPI awareness, with Per-Monitor fallback.
Display enumeration uses `EnumDisplayMonitors` and `GetMonitorInfoW`; it reads
the current layout without changing system display modes.

Linux and macOS currently implement path discovery only. The platform-neutral
window API is in place, but those native window backends still need their own
implementations before the game can open a window on those systems.

Use native `PathBuf` and `OsStr` values rather than assuming paths are UTF-8.
Keep raw OS handles private to backend code. Future capabilities such as
clipboard or controller input should use narrow, owned APIs behind the same
facade.

## Threading and lifecycle

`ApplicationPlatform` is owned by the application service container. The
event emitter is cloneable for subsystem producers. Queue synchronization
protects sequence assignment and insertion together, so sequence values match
enqueue order when producers contend. The queue is bounded to prevent
unbounded growth.

Windows HWNDs must be created, pumped, and destroyed on their owner thread.
`ApplicationRuntime` polls native messages each frame; the game destroys its
main window during orderly shutdown. `WindowSystem` also destroys any remaining
windows when dropped on the owner thread. Keep window procedure callbacks short
because they block message processing on that thread.

Window and lifecycle events are dispatched through the `nord_app` event facade
on the application thread. Keyboard and pointer events are delivered to the
input facade, which tracks held state per window and clears it on focus loss.
Game modules subscribe through these application-facing facades; they do not
depend on `nord_events`, `nord_inputs`, or native message types directly.

The platform object does not open files, create directories, or start threads.
Registering the Win32 window class and creating an HWND happen only after the
application explicitly requests a window.

## Technical references

- [Rust conditional compilation](https://doc.rust-lang.org/reference/conditional-compilation.html)
- [`std::time::Instant`](https://doc.rust-lang.org/stable/std/time/struct.Instant.html)
- [Microsoft: creating a window](https://learn.microsoft.com/en-us/windows/win32/learnwin32/creating-a-window)
- [Microsoft: writing the window procedure](https://learn.microsoft.com/en-us/windows/win32/learnwin32/writing-the-window-procedure)
- [Microsoft: setting process DPI awareness](https://learn.microsoft.com/en-us/windows/win32/hidpi/setting-the-default-dpi-awareness-for-a-process)
- [Microsoft: enumerating display monitors](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumdisplaymonitors)
- [Microsoft: retrieving monitor information](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getmonitorinfow)
- [Microsoft: finding a monitor for a window](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-monitorfromwindow)
- [`windows-sys` Windows message APIs](https://docs.rs/windows-sys/0.61.2/windows_sys/Win32/UI/WindowsAndMessaging/index.html)
