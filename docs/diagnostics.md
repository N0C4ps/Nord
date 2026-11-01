# Nord diagnostics

## Boundary and API

`crates/nord_diagnostics` centralizes application-level error context and
invariant assertions. `crates/nord_app` owns the public facade exposed to the
game. Code in `src/` imports `ApplicationResult`, `ApplicationErrorContext`,
`ensure!`, and invariant macros from `nord_app`; it does not depend directly on
`nord_diagnostics` or `anyhow`.

The crate uses `anyhow` for application composition: errors can accumulate
human-readable operation context while retaining the original source chain.
Reusable Nord libraries should keep concrete error enums/structs in their public
APIs, so callers can match on expected cases. `thiserror` is suitable for
implementing those standard error types without exposing the derive crate as
part of the API.

## Recoverable failures

Return `ApplicationResult<T>` for application operations that can fail. Add
context where the caller knows what operation was attempted, then propagate the
error with `?`:

```rust
use nord_app::{ApplicationErrorContext, ApplicationResult};

fn load_game_settings() -> ApplicationResult<GameSettings> {
    let settings = read_settings_file()
        .context("reading the game settings file")?;
    parse_game_settings(&settings)
        .context("parsing the game settings file")
}
```

Use `ensure!` for a failed precondition that the caller can handle, `bail!` to
return an error from the current operation, and `ensure_eq!`/`ensure_ne!` when
the compared values make a failure easier to diagnose. These are error returns;
they do not panic and do not emit logs themselves. The application boundary
decides whether recovery is possible and logs the resulting failure once.

Avoid logging and returning the same error at every layer. A lower-level
function should add context only when it knows useful detail; the boundary with
enough information to decide the outcome emits the WARN or ERROR. Failures
before the logging pipeline is available use standard error as the bootstrap
fallback.

## Assertions and invariants

- `debug_invariant!` checks an internal invariant in debug builds. Its condition
  and formatting arguments are omitted from release builds without debug
  assertions. Do not put side effects in the condition.
- `invariant!` checks an internal invariant in every build. A violation
  panics, which the logger's panic diagnostics can capture. Use it only when
  continuing could corrupt state or make subsequent behavior meaningless.
- Use `ensure!`/`bail!` for expected environmental or input failures, including
  invalid files, unsupported settings, resource exhaustion, and user-provided
  data. Assertions are for programmer invariants, not normal invalid input.

The application lifecycle uses `debug_invariant!` to catch invalid state
transitions during development. The game loop uses `ensure!` for iteration
counter exhaustion and propagates that failure to the application boundary.

## Research references

- [The Rust Book: Recoverable Errors with `Result`](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html)
  describes returning and propagating errors so a caller with more context can
  decide how to respond.
- [`anyhow`](https://docs.rs/anyhow/latest/anyhow/) provides application error
  context, source chains, and the `bail!`/`ensure!` family.
- [`thiserror`](https://docs.rs/thiserror/latest/thiserror/) derives standard
  library error implementations while keeping the concrete error type in a
  library's public API.
- [Rust standard library assertions](https://doc.rust-lang.org/std/macro.debug_assert.html)
  distinguish always-on assertions from debug-only checks.
