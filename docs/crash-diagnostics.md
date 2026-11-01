# Nord crash diagnostics

## Ownership and integration

`crates/nord_crash` owns the process panic hook, local panic reports, and the
optional native minidump monitor. The crate does not depend on `nord_log`.
`nord_app` exposes it through `ApplicationCrashReporter`, and the game registers
that service next to `ApplicationLogger` during bootstrap.

The logger provides only a bounded `panic_flush_callback`. The panic hook calls
it before persisting its report, waiting at most 250 ms for queued local log
records to flush. This preserves recent logs while keeping crash capture
independent from the logger's implementation.

## Captured information

The reporter chains the existing Rust panic hook and appends one structured
JSONL record to the configured `logs/crashes.jsonl` file. Each report includes
the panic payload, source file/line/column, thread ID and name, timestamp,
application name/version, and an optional forced backtrace. It flushes and
syncs the file before returning from the hook. Reports stay local; the crash
crate does not upload them.

When enabled, `minidumper-child` starts a separate monitor process. It writes
native minidumps under `logs/dumps/`, renames them to stable `nord-*.dmp` names,
and exits when the game process exits. A monitor startup failure is non-fatal:
panic JSONL reports stay enabled and `ApplicationCrashReporterDiagnostics`
contains the failure. The app logs one startup warning for that fallback.

Release builds retain level-1 debug information and matching PDBs under
`target/release/`; keep those PDBs with the exact executable build to
symbolicate native Windows stacks.

## Error boundaries and limits

Failure to create the crash report file or to install Nord's process-global
panic hook aborts application initialization with a source-preserving error.
Native monitor failure is a recoverable degraded mode and becomes a warning.
The panic hook does not recover execution. Rust panic hooks run before the
panic runtime, including `panic = "abort"`, but a hook cannot capture power
loss, forced termination, hardware failure, or every stack-overflow path.

Panic hooks are process-global and are installed once. The application must
retain `ApplicationCrashReporter` for its process lifetime. The optional flush
callback must remain bounded and must not panic.

## References

- [`std::panic::set_hook`](https://doc.rust-lang.org/std/panic/fn.set_hook.html)
  describes the process-global hook and when it runs relative to the panic
  runtime.
- [`PanicHookInfo`](https://doc.rust-lang.org/std/panic/struct.PanicHookInfo.html)
  exposes the panic payload and source location supplied to the hook.
- [`minidumper-child`](https://docs.rs/minidumper-child/latest/minidumper_child/)
  starts and owns the out-of-process native monitor.
- [`minidumper`](https://docs.rs/minidumper/latest/minidumper/struct.Client.html)
  documents requests to generate a dump from the crashing process context.
- [Windows Error Reporting settings](https://learn.microsoft.com/en-us/windows/win32/wer/wer-settings)
  documents LocalDumps configuration. Nord uses its cross-platform monitor
  instead of requiring machine-level WER registry configuration.
