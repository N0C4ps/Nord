# Configuration system

`nord_config` is the engine's format-independent configuration loader. The
game defines its own typed schema in `src/app/config.rs`; `nord_app` re-exports
the loader as the stable application facade. This keeps the game independent
from the loader implementation and lets each engine crate own only its own
configuration types.

## Sources and precedence

The loader starts with typed defaults and then applies these layers:

1. Defaults serialized from the schema's Rust `Default` implementation.
2. JSON or TOML files, in the order they are added to the loader.
3. Process environment variables matching the configured prefix.
4. Explicit environment overrides supplied by an embedding application.

Objects/tables merge recursively. Arrays and scalar values replace the earlier
value. Missing optional files are ignored; malformed, unreadable, or unsupported
files are errors. Deserialization and application validation happen after all
sources have been merged, so consumers only receive a complete, validated
configuration.

Nord loads `%APPDATA%/Nord/nord.toml` on Windows through the platform-provided
configuration directory. If it is absent, the typed defaults are used. Set
`NORD_CONFIG_FILE` to load a required JSON or TOML file from another path. The
tracked [`nord.example.toml`](../config/nord.example.toml) and
[`nord.example.json`](../config/nord.example.json) demonstrate the current
game-owned schema in both supported formats.

## Environment variables

Double underscores separate nested field names. For example:

```powershell
$env:NORD__APPLICATION__TITLE = 'Nord: Sandbox'
$env:NORD__WINDOW__WIDTH = '1600'
$env:NORD__WINDOW__HEIGHT = '900'
$env:NORD__TIMING__MAX_FRAME_DELTA_MS = '250'
$env:NORD__TIMING__TIME_SCALE = '1.0'
```

The loader treats JSON scalar syntax as typed values, so numbers, booleans,
arrays, objects, and quoted strings can be represented. A plain non-JSON value
is treated as a string. Invalid paths, incompatible merges, unknown fields,
type mismatches, and values rejected by the game schema fail startup with
source context; config values are not silently discarded.

## Formats

TOML is the default human-edited file format for the game. The engine loader
also supports JSON for tools, generated configuration, and deployments. File
format is inferred case-insensitively from `.toml` or `.json`; callers can set
the format explicitly for extensionless files. TOML and JSON deserialize into
the same Serde schema, so format choice does not change game configuration
semantics.

## Current game schema

| Setting | Default | Validation |
| --- | ---: | --- |
| `application.title` | `Nord` | Non-empty, up to 128 bytes |
| `window.width` | `1280` | Positive client width in pixels |
| `window.height` | `720` | Positive client height in pixels |
| `window.resizable` | `true` | Boolean |
| `threading.worker_threads` | available parallelism minus one, minimum one | Positive integer when set |
| `tasks.queue_capacity` | `2048` | Positive integer |
| `tasks.max_in_flight` | worker-pool size | Positive integer when set; cannot exceed worker count |
| `tasks.priority_aging_interval_ms` | `250` | Positive integer |
| `timing.max_frame_delta_ms` | `250` | 1–60,000 |
| `timing.fixed_step_ns` | `16,666,667` | 1–10,000,000,000 |
| `timing.max_steps_per_frame` | `8` | 1–1,000 |
| `timing.time_scale` | `1.0` | Finite, 0–100 |

Unknown fields are denied at each schema level, and omitted fields inherit
defaults. The validated timing values configure the runtime frame clock and
fixed-step simulation. Configuration hot reload, profile selection, remote
providers, and secret stores are intentionally future capabilities; they can be
added behind the loader/facade without coupling `src/` to a parser crate.
