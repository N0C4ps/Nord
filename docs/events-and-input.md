# Events and input

## Ownership

`nord_events` owns the normalized application/window event contracts and a
typed listener dispatcher. `nord_inputs` owns normalized keyboard/pointer
events, held-state tracking, and input subscriptions. `nord_platform` is the
native producer: its backend translates operating-system messages into those
contracts and places them in the existing bounded, timestamped platform event
queue. `nord_app` exposes the stable `ApplicationEventSystem` and
`ApplicationInputSystem` facades; game modules use those facades rather than
depending on implementation crates.

## Dispatch and lifecycle

The application runtime pumps native messages on the window-owning thread,
drains the platform queue in sequence order, then sends window/lifecycle events
to `ApplicationEventSystem` and keyboard/pointer events to
`ApplicationInputSystem`. Typed subscriptions run in registration order from a
stable snapshot. Dispatch releases internal locks before invoking callbacks,
and subscription tokens unregister listeners on drop. Listener panics are
counted and reported to the runtime when the active build uses panic unwinding.
The optimized game profiles use `panic = "abort"`; there, a listener panic
terminates the process and is handled by crash diagnostics rather than resumed
dispatch.

The native queue is bounded at 4096 entries and event sequence/timestamps are
assigned when producers enqueue. Native callbacks only translate small
messages; they do not invoke gameplay callbacks. On focus loss, the input
system clears held state and sends synthetic releases so a key or button does
not remain stuck after the application becomes inactive.

## Current input behavior

The Windows backend currently normalizes focused-window key down/up, repeat,
modifier, pointer movement, primary/secondary/middle/auxiliary buttons, and
vertical/horizontal wheel messages. Key and pointer events carry their source
window. Wheel deltas are expressed in standard detents. Input state is tracked
per window and can be queried without subscribing to every frame's events.

Raw Input is not registered yet. It is the extension point for device identity,
unaccelerated relative mouse motion, and high-polling-rate mice. Registration is
an application-level choice on Windows; a library should not silently take
ownership of the process's raw-input registration. The initial game runtime
therefore uses ordinary focused-window messages and keeps the native input API
replaceable behind `nord_app`.

## References

- [Microsoft: keyboard input overview](https://learn.microsoft.com/en-us/windows/win32/inputdev/about-keyboard-input)
- [Microsoft: virtual-key codes](https://learn.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes)
- [Microsoft: Raw Input overview](https://learn.microsoft.com/en-us/windows/win32/inputdev/about-raw-input)
- [Microsoft: using Raw Input](https://learn.microsoft.com/en-us/windows/win32/inputdev/using-raw-input)
- [Microsoft: `WM_INPUT`](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-input)
