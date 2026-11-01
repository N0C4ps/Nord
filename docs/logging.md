# Nord logging

## Ownership and API boundary

`crates/nord_log` implements the process-wide logging pipeline. `crates/nord_app`
owns its application facade and installs it during composition. Game modules
emit events through the macros re-exported from `nord_app`; they do not depend
directly on `nord_log` or its implementation crates. Engine libraries may use
the standard `tracing` or `log` facades; the Nord subscriber also bridges
`log` records into the same pipeline.

## Event path and latency

The logger uses `tracing` events and spans. At an enabled callsite the producer
captures metadata, typed event fields, the current span fields, source file and
line, thread ID/name, wall-clock timestamp, and elapsed time into an owned
record. The record enters one bounded multi-producer queue. A dedicated worker
assigns the output sequence, renders readable terminal text, serializes JSON,
and writes to the configured sinks. A single worker preserves one output order
across threads and fans each record to terminal and file sinks.

The hot path does not perform terminal/file I/O, JSON serialization, or dynamic
string formatting for common short fields. Event fields, field lists, and span
stacks use inline storage (48 bytes per short string, eight fields, and eight
spans before growing); only longer text or unusually large events spill to the
heap. Events still cost CPU to visit and copy active context. Disabled log
levels are filtered before the visitor runs. Avoid per-voxel/per-particle
events every frame; aggregate them or use a metrics facility for high-frequency
data.

## Configuration and outputs

`ApplicationLoggerConfig` supports readable terminal output and an optional
JSON Lines file sink. The file sink can rotate daily, hourly, weekly, or never,
with a configurable retention count. The filter can be set explicitly or read
from `NORD_LOG`, then `RUST_LOG`, with `info` as the default. The queue capacity
and full-queue policy are configurable.

The game currently writes terminal output and a daily JSONL file under the
platform data directory's `logs/` folder. It retains 14 files by default.
Crash reports and native minidumps are owned by the separate `nord_crash`
crate; see [crash-diagnostics.md](crash-diagnostics.md). The logger exposes a
bounded callback to flush its local sinks before that crate writes a panic
report, without owning the panic hook or native monitor.

OTLP/HTTP export is enabled when a standard logs, traces, or generic OTLP
endpoint variable is set. A generic endpoint accepts a Collector base URL such
as `http://localhost:4318`; Nord appends `/v1/logs` and `/v1/traces`. Signal
specific endpoint variables are used as supplied.
Nord uses HTTP/protobuf batch exporters for both logs and traces, so network
export runs on SDK-managed background work. The `tracing` spans are converted
to OpenTelemetry spans and log records carry their trace/span identifiers.
When setting a protocol environment variable, use `http/protobuf`; unsupported
protocols produce a startup error instead of silently sending to the wrong
transport. `ApplicationLogger::flush` waits for the local sinks and both OTLP
batches, with a two-second exporter timeout. Configure it directly with
`ApplicationOpenTelemetryConfig::otlp_http` if preferred.

### Local Collector check

`scripts/otel-collector.yaml` is a loopback-only Collector configuration for
checking both OTLP signals without sending data to an external service. With an
OpenTelemetry Collector binary available locally, run it in one terminal:

```powershell
otelcol.exe --config .\scripts\otel-collector.yaml
```

Then run Nord in another terminal:

```powershell
$env:OTEL_EXPORTER_OTLP_ENDPOINT = 'http://127.0.0.1:4318'
$env:OTEL_EXPORTER_OTLP_PROTOCOL = 'http/protobuf'
.\scripts\run_debug.ps1
```

The Collector's `debug` exporter should show both `application_start` under
`ResourceLogs` and the `application` span under `ResourceSpans`, with
`service.name=nord` and matching trace/span IDs. On 2026-10-30 this was checked
against the official OpenTelemetry Collector Windows binary v0.164.0. Logs and
traces were both received; the logger reported no telemetry export errors.
Keep the Collector and Nord in the same Windows security/network context when
running the local check, because process isolation can prevent loopback IPC.

## Backpressure and shutdown

The default queue holds 16,384 events and drops new events if it is full; this
keeps a slow disk or terminal from indefinitely blocking the simulation.
`LoggerDiagnostics` counts dropped events, sink errors, and explicit OTLP flush
failures. `Block` is available
when loss is unacceptable, but it can stall producer threads during a sink
outage. The game checks and reports logger diagnostics after its loop.

The worker flushes sinks periodically and `ApplicationLogger::flush` supports
an explicit timeout. The application container keeps `ApplicationLogger` alive
for the process lifetime. Dropping the runtime shuts down the OTLP batch
processor, drains queued events, flushes the sinks, and joins the worker.

## Structured context

Each record includes timestamp, severity text and OpenTelemetry-compatible
severity number, event name, target/instrumentation scope, event fields, source
location, thread identity, and current span stack. The application creates a
root span with game name/version, process ID, and active platform. Use spans to
attach stable context to operations, and use `#[nord_app::instrument]` or
`Instrument` when work moves to another task/thread; thread-local span context
does not cross arbitrary thread boundaries automatically.
When an event supplies a string field named `event_name`, the logger promotes
it to the record's event name instead of keeping a duplicate attribute.

The record shape follows the OpenTelemetry log data model closely, including
timestamps, severity, body/attributes, resource-like span context, and source
scope. When OTLP is enabled, the standard tracing bridge maps events and active
trace context into OpenTelemetry log records and associates the `nord` service
resource. The tracing layer also exports spans and generates the trace IDs used
to correlate each log event.

## Crash diagnostics integration

`nord_log` remains responsible for asynchronous logs. Its bounded
`panic_flush_callback` lets `nord_crash` ask the log worker to flush before the
crash report is synchronously persisted. Details and limits of panic and native
crash capture are documented in [crash-diagnostics.md](crash-diagnostics.md).

## Research references

- [Quill overview](https://quillcpp.readthedocs.io/en/latest/overview.html):
  separate latency-sensitive event capture from backend formatting and I/O,
  with thread-local queues and explicit overflow behavior.
- [spdlog asynchronous logging](https://github.com/gabime/spdlog/wiki/Asynchronous-logging):
  bounded queues, explicit block/drop policies, and the ordering tradeoff when
  multiple workers are used.
- [Serilog enrichment](https://github.com/serilog/serilog/wiki/Enrichment):
  attach execution context to events instead of repeating it in each message.
- [OpenTelemetry log specification](https://opentelemetry.io/docs/specs/otel/logs/)
  and [log data model](https://opentelemetry.io/docs/specs/otel/logs/data-model/):
  resource/scope context, severity, timestamps, attributes, and trace
  correlation fields.
- [`tracing`](https://docs.rs/tracing/latest/tracing/) and
  [`tracing-subscriber`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/):
  Rust structured events, spans, subscriber layers, filters, and log bridging.
- [Rust panic hook](https://doc.rust-lang.org/std/panic/fn.set_hook.html):
  panic hooks execute before the panic runtime and receive payload/location
  information; the hook is a process-global resource.
- [`opentelemetry-appender-tracing`](https://docs.rs/opentelemetry-appender-tracing/latest/opentelemetry_appender_tracing/)
  and [`opentelemetry-otlp`](https://docs.rs/opentelemetry-otlp/latest/opentelemetry_otlp/):
  bridge tracing events to OpenTelemetry logs and batch-export them using OTLP.
- [OpenTelemetry Collector Docker installation](https://opentelemetry.io/pl/docs/collector/install/docker/)
  and [Windows binary installation](https://opentelemetry.io/uk/docs/collector/install/binary/windows/):
  official Collector setup references used for the real OTLP integration check.
