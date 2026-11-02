# Threading, jobs, and task scheduling

`nord_threading` owns the worker-pool lifecycle and exposes an opaque,
configurable pool. `nord_jobs` exposes data-parallel operations over slices,
owned items, and independent task joins. The game creates one pool during
startup and uses it for render-graph callback encoding and D3D12 barrier and
command-list recording. `nord_scheduler` remains available as a crate, but the
current game does not construct it because there are no independently
cancellable background tasks to submit yet.

## First scheduler: Rayon

Rayon provides the pool's work-stealing parallel iteration and fork-join
execution; the game does not maintain a second worker pool for rendering. The
engine creates an owned pool rather than configuring the process-global pool,
making its lifetime and worker count explicit. The pool is constructed once in
application startup and shared through the job-system facade.

By default, the pool uses the standard library's available-parallelism estimate
minus one logical processor, with a minimum of one worker. This leaves capacity
for the application thread when possible. Set `threading.worker_threads` in
`nord.toml` or `NORD__THREADING__WORKER_THREADS` to override it. Worker names use
the `nord-worker-N` prefix and stack sizes remain at platform defaults unless a
host configures them explicitly.

## Job API

`ApplicationJobSystem` currently offers:

- parallel read-only `for_each` over a slice;
- parallel disjoint `for_each_mut`;
- parallel `map`, whose collected output follows input order;
- owned-item `map_owned`, whose results follow input order;
- indexed `for_each_chunk_mut` for independent chunk-sized data;
- `join` for two independent computations.

The current renderer is a direct consumer: it encodes independent render-pass
callbacks and records per-pass D3D12 barriers and command lists on this pool.
This reuses persistent workers instead of creating scoped OS threads for each
render-graph submission. Small callback sets stay on the calling thread to
avoid dispatch overhead.

Work execution order and worker identity are unspecified. Jobs should be
CPU-bound, reasonably sized, and independent where possible. Rust's `Send` and
`Sync` bounds prevent data races at the API boundary; mutable slices are divided
into disjoint regions by the scheduler. Avoid blocking waits, long synchronous
I/O, and locks with long critical sections inside worker operations, since they
reduce throughput and can stall all work sharing the pool.

## Nord task scheduler (not started by the game yet)

Rayon does not expose priority scheduling or task cancellation. `nord_scheduler`
therefore owns a bounded, multi-priority waiting queue and dispatches up to a
configured in-flight limit into the shared Rayon pool. A dedicated dispatcher
keeps queue policy outside worker execution. Queue admission is non-blocking:
when capacity is exhausted, `submit` returns `QueueFull` so the caller can
retry, lower priority, or discard stale work without stalling a frame.

Priorities are `Critical`, `High`, `Normal`, `Low`, and `Background`. They affect
queued work only; running CPU tasks are never preempted. FIFO order is preserved
within each priority. Priority aging promotes waiting work at a configurable
interval, preventing a constant stream of foreground jobs from starving old
background tasks. This is a scheduling preference, not a hard real-time
deadline guarantee.

Each accepted task gets a stable scheduler-local ID and a completion handle.
Dropping the handle only drops result observation. `cancel()` prevents a queued
task from starting or signals a running task through its `TaskContext`; running
work must call `checkpoint()` or inspect its cancellation token at safe points.
Synchronous CPU work cannot be safely force-killed in Rust, so cancellation is
cooperative. The handle reports cancellation, panic (when panic unwinding is
enabled), or a result; production's `panic = "abort"` policy still treats a
panic as process-fatal and lets crash diagnostics handle it.

Shutdown stops submissions and can either drain queued work or cancel pending
work, then joins the dispatcher and waits for running tasks. It must be called
from outside the scheduler's worker pool. Implicit drop cancels pending work and
detaches dispatcher cleanup to avoid blocking a worker destructor; application
shutdown calls `CancelPending` explicitly before draining logs.

An independently cancellable operation uses the application facade and polls
at safe boundaries:

```rust
let handle = scheduler.submit(ApplicationTaskPriority::Background, move |context| {
    let mut result = Vec::new();
    for input in inputs {
        context.checkpoint()?;
        result.push(build_part(input));
    }
    Ok::<_, ApplicationTaskExecutionError>(result)
})?;

// Later, poll without blocking the game thread. The outer Result represents
// scheduler cancellation/shutdown; the inner Result is the task's own outcome.
if let Some(task_result) = handle.try_take()? {
    let built_parts = task_result?;
}
```

Task closures should be CPU-bound and coarse enough to amortize queue/handle
overhead. Use `nord_jobs` for fine-grained parallel work. Add the scheduler to
the game bootstrap only when a real background workload needs independent
cancellation or priority, such as cancellable asset preparation. The current
platform event loop stays sequential because quit and lifecycle events are
ordered control flow, not a parallel data workload.

## Sources

- [Rayon crate documentation](https://docs.rs/rayon/latest/rayon/)
- [Rayon thread pool builder](https://docs.rs/rayon/latest/rayon/struct.ThreadPoolBuilder.html)
- [Rayon scoped tasks and panic behavior](https://docs.rs/rayon/latest/rayon/fn.scope.html)
- [Rust `available_parallelism`](https://doc.rust-lang.org/std/thread/fn.available_parallelism.html)
- [Tokio task cancellation semantics](https://docs.rs/tokio/latest/tokio/task/)
- [Crossbeam bounded queues and non-blocking submission](https://docs.rs/crossbeam-channel/latest/crossbeam_channel/)
