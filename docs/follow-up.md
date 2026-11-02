# Follow-up records

## Resolved: structured logging during application bootstrap

The application now initializes the structured logger and panic reporter before
loading game configuration or creating the native window. Failures after the
logger starts are recorded as `application_bootstrap_failed`, and the executable
does not print those errors a second time to stderr.

Platform initialization, OpenTelemetry environment parsing, and logger
initialization still use the executable's stderr fallback because the logging
pipeline is not available yet.
