# otel-wasi

OpenTelemetry tracing layer for WASI and WebAssembly component entry points.

`otel-wasi` gives Rust component authors a single attribute macro, `#[wasi_instrument]`, that sets up tracing, creates a root span for one invocation, enters it, records duration, and preserves parent trace context. Child functions keep using normal `tracing` instrumentation.

## Why this exists

Adding OpenTelemetry to every WASI component handler means repeating the same setup: initialize the SDK, extract trace context, start a span, record attributes, match on the result, and finish with the right status. `otel-wasi` removes that boilerplate for the entry point while keeping the runtime API explicit and replaceable.

This project is aimed at authors of wasmCloud components, `wasi:http` handlers, and other WebAssembly component runtimes that propagate trace context through WASI.

## Quick start

### Prerequisites

- Rust toolchain (the runtime and macro crates build on stable; the dylint lint crate needs nightly).
- A WASI component runtime such as wasmCloud or wasmtime if you want to run the built components.

### Add the dependency

`otel-wasi` is not yet published on crates.io. Add it as a git dependency:

```toml
[dependencies]
otel-wasi = { git = "https://github.com/Seamlezz/otel-wasi" }
```

The default feature enables the `#[wasi_instrument]` macro.

### Minimal example

```rust
use otel_wasi::wasi_instrument;

#[wasi_instrument(service = "my-service")]
fn do_work() -> Result<(), otel_wasi::Error> {
    otel_wasi::attribute!("work.unit" = "demo");
    Ok(())
}
```

The macro creates the root span, records the attribute, and marks the span as OK on success or error on failure.

### Verify it builds

```sh
cargo check
cargo test -p otel-wasi -p otel-wasi-macros
```

## Usage

### WASI component exports

For exported component handlers, use `export` so the macro rewrites the WIT signature and converts `otel_wasi::Error<E>` back to the underlying WIT error type `E`.

```rust
#[otel_wasi::wasi_instrument(
    service = "nats-echo",
    name = "handle-message",
    export,
    attributes("messaging.system" = "nats")
)]
fn handle_message(msg: BrokerMessage) -> Result<(), otel_wasi::Error> {
    otel_wasi::attribute!(
        "messaging.destination.name" = msg.subject.clone(),
        "messaging.message.body.size" = msg.body.len() as i64,
    );

    publish_reply(&msg)
}
```

For typed WIT errors such as `wasi:http/handler@0.3.0`'s `ErrorCode`, carry the payload through `otel_wasi::Error<ErrorCode>`:

```rust
#[otel_wasi::wasi_instrument(
    service = "http-hello",
    export,
)]
async fn handle(request: Request) -> Result<Response, otel_wasi::Error<ErrorCode>> {
    // body returns Result<Response, otel_wasi::Error<ErrorCode>>
    fallible_http_operation().error_with_slug("http-hello-op-failed")?
}
```

### Child functions

Inside a `#[wasi_instrument]` function, use normal `#[tracing::instrument]` for children. `attribute!` writes to the current span, and `main_attribute!` writes to the root `#[wasi_instrument]` span:

```rust
#[tracing::instrument(name = "publish-reply", level = "info", skip(msg))]
fn publish_reply(msg: &BrokerMessage) -> Result<(), otel_wasi::Error> {
    // detail for the child span
    otel_wasi::attribute!(
        "messaging.message.body.size" = msg.body.len() as i64,
    );

    // rollup context on the entrypoint span
    otel_wasi::main_attribute!(
        "messaging.reply.publish_attempted" = true,
    );

    Ok(())
}
```

`main_attribute!` outside an active `#[wasi_instrument]` invocation triggers a debug assertion and is a no-op in release builds.

### Error slugging

`otel_wasi::Error` pairs a stable slug with a human readable message. Create one with `wasi_error!`:

```rust
let err = otel_wasi::wasi_error!(
    "db-timeout",
    "connection to {} timed out after {}ms",
    host,
    ms,
);
```

Attach slugs to existing errors:

```rust
use otel_wasi::{ResultWithSlug, WithSlug};

let result = fallible_op().error_with_slug("fallible-op-failed");
let err = io_error.with_slug("file-read-failed");
```

## Core concepts

- **Root span**: `#[wasi_instrument]` owns the main span for one component invocation. It sets `service.name`, `service.version`, an optional `service.build.git_hash`, and the `trace_id`.
- **Context propagation**: the macro extracts the parent trace context from the WASI runtime before entering the span.
- **Outcome recording**: return values are classified via `SpanOutcome`. `()` always records OK. `Result<T, E>` records OK or error only when `E` implements `WasiError`. For other return types, call `finish_ok()` or `finish_error()` with the manual API.
- **Export mode**: `export` rewrites the function signature from `Result<T, otel_wasi::Error<E>>` to `Result<T, E>`. The body still uses `otel_wasi::Error<E>`, and the macro extracts the inner `E` at the boundary.
- **Async entry points**: native `async fn` is supported. The span is entered only while the future is polled, using `tracing::Instrument`. Manual future wrappers or `async-trait` generated functions are not special-cased.

## Configuration

### `#[wasi_instrument]` options

| Option | Required | Description |
| --- | --- | --- |
| `service = "..."` | yes | Sets the OpenTelemetry service name resource. |
| `name = "..."` | no | Sets the span name. Defaults to the function name with underscores replaced by hyphens. |
| `export` | no | Enables WIT export signature rewriting for component exports. |
| `attributes("key" = value, ...)` | no | Static root span attributes. |

### Compile time attributes

Set `GIT_HASH` at compile time to record `service.build.git_hash`:

```sh
GIT_HASH=$(git rev-parse HEAD) cargo build
```

### Manual API

The macro is the preferred API, but the runtime stays usable directly:

```rust
let tracing_span = otel_wasi::span!(tracing::Level::INFO, "handle-message");
let config = otel_wasi::SpanConfig::builder()
    .service_name("nats-echo")
    .span_name("handle-message")
    .build();
let span = otel_wasi::WasiSpan::from_span(tracing_span, config);

let result = {
    let _main_guard = otel_wasi::enter_main_span(span.span().clone());
    let _guard = span.enter();
    publish_reply(&msg)
};

span.finish(&result);
```

For returns that are not a `Result`, call `span.finish_ok()` or `span.finish_error("slug", "message")`.

## Examples

- [`examples/wasmcloud-nats-echo`](examples/wasmcloud-nats-echo): synchronous wasmCloud messaging handler with a string WIT error.
- [`examples/wasmcloud-http-hello`](examples/wasmcloud-http-hello): asynchronous `wasi:http/handler@0.3.0` handler with a typed `ErrorCode` error.

## Lint: `slug_on_wasi_error`

`otel-wasi` ships a [dylint] lint that denies calling `with_slug` or `error_with_slug` on types that already implement `WasiError`. Double wrapping silently drops the original slug. See [`docs/dylint.md`](docs/dylint.md) for installation, workspace configuration, and IDE setup.

[dylint]: https://github.com/trailofbits/dylint

## Development

This repository is a Cargo workspace:

```text
otel-wasi/
  otel-wasi/          runtime library
  otel-wasi-macros/   proc macro crate
  otel-wasi-dylint/   dylint lint crate
  examples/           wasmCloud component examples
```

Run tests for the runtime and macros:

```sh
cargo test -p otel-wasi -p otel-wasi-macros
```

The dylint crate needs a nightly toolchain with `rustc-dev` and `llvm-tools`:

```sh
cargo test -p otel-wasi-dylint
```

## Status

This crate is under active development. The current workspace version is `0.2.0`. It is not yet at version 1.0; breaking changes are possible until a stable release. It is not yet recommended for production without further review.

## License

Licensed under the GNU Affero General Public License v3.0. See [LICENSE](LICENSE) for the full text.
