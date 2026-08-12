# ADR 0008: Explicit Tauri async runtime

## Status

Accepted

## Context

The core engine owns a background operation worker. Its constructor previously
called `tokio::spawn`, which works in Tokio tests and the async CLI but panics
when the native desktop creates services inside Tauri's synchronous `setup`
callback. Tauri has an async runtime, but that callback is not entered into its
Tokio reactor.

## Decision

`Engine::new` requires an explicit `tokio::runtime::Handle` and schedules the
worker through that handle. The desktop passes
`tauri::async_runtime::handle().inner()`, while async tests and the CLI pass the
current Tokio handle. A unit test constructs the engine outside an entered
runtime to cover the native startup condition.

## Consequences

- Native Tauri startup no longer depends on an implicit thread-local reactor.
- Every engine caller must state which runtime owns the background worker.
- Core tests continue to use Tokio directly without adding a Tauri dependency.
