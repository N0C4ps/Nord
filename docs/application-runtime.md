# Nord application runtime

## Boundary and ownership

`src/app/` composes the game-owned application from the stable `nord_app`
facade. It owns startup, runtime control, lifecycle transitions, shutdown, and
the application-level error boundary. `crates/` continues to own reusable
engine infrastructure; the game loop in `src/gameloop/` is where Nord's
simulation and presentation systems will be connected as they are built.

## Current lifecycle

The initial runtime follows these phases:

```text
Bootstrapping -> Running -> ShuttingDown -> Stopped
                     \-> Failed -> ShuttingDown -> Stopped
```

Startup composes platform, logger, and game settings. The runtime installs one
cross-platform Ctrl+C handler, polls normalized platform events every 16 ms,
and advances the game-owned loop. Ctrl+C, a PAL quit event, or a future window
close event requests an orderly shutdown. The signal callback only sets an
atomic flag; normal application code performs logging and cleanup.

The runtime is currently headless: there is no native window, renderer,
simulation, fixed-timestep scheduler, or input handling yet. The poll cadence is
only for keeping the app responsive to shutdown and platform events; it is not
the game's simulation rate. Do not emit one log per input or loop iteration.

## Error policy

Fallible operations return `Result` and retain their source error. Lower-level
functions add context when they own useful context, then propagate with `?`;
they do not log and return the same failure at every layer. The application
boundary logs a fatal runtime failure once, completes orderly shutdown, and
returns a nonzero process exit code. Bootstrap failures that occur before the
logger is available are reported once to standard error. This avoids pretending
that the logger can report its own initialization failure.

Use `WARN` for a problem the game recovered from or a degraded fallback it
actually chose. Use `ERROR` when an operation failed and the application cannot
continue that path. Expected user actions and ordinary shutdown are not errors.
Rust `Result` is the recoverable error path; panic is reserved for violated
invariants or other unrecoverable programmer failures.

The logger emits sparse lifecycle events (`application_started`,
`application_stopping`, `application_stopped`) with structured fields. The
application keeps the logger alive through shutdown, checks its diagnostics,
and requests a final flush before its service container is dropped.

## References

- [The Rust Book: Recoverable Errors with `Result`](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html)
  explains propagation and handling errors at the layer with enough context.
- [`ctrlc::set_handler`](https://docs.rs/ctrlc/latest/ctrlc/fn.set_handler.html)
  documents the cross-platform Ctrl+C handler and its single-registration
  process-level behavior.
