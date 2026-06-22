# otel-wasi

`otel-wasi` is a small developer-experience layer for adding OpenTelemetry tracing to WASI / WebAssembly component entry points.

The goal is to make the common path feel like this:

```rust
use otel_wasi::wasi_instrument;

#[wasi_instrument(service = "nats-echo")]
fn handle_message(msg: BrokerMessage) -> Result<(), String> {
    publish_reply(&msg)
}
```

The macro sets up tracing, creates the main WASI span, enters it, records duration, records success/error status, and preserves parent trace context. Native `async fn` entry points are supported; async-trait/manual future-returning wrappers are not currently special-cased.

## Intended crate layout

This repository is a workspace:

```text
otel-wasi/
  otel-wasi/          # runtime library
  otel-wasi-macros/   # proc macro crate
```

The runtime crate owns the real behavior. The macro crate should stay thin and only generate calls into the runtime API.

## Main span instrumentation

`#[wasi_instrument]` is intended for WASI/component entry points — the main/root span for one invocation.

```rust
#[otel_wasi::wasi_instrument(
    service = "nats-echo",
    name = "handle-message",
    error_slug = "nats-publish-failed",
    attributes(
        "messaging.system" = "nats"
    )
)]
fn handle_message(msg: BrokerMessage) -> Result<(), String> {
    otel_wasi::attribute!(
        "messaging.destination.name" = msg.subject.clone(),
        "messaging.message.body.size" = msg.body.len() as i64,
    );

    publish_reply(&msg)
}
```

Child functions should generally use normal `tracing` instrumentation:

```rust
#[tracing::instrument(name = "publish-reply", level = "info", skip(msg))]
fn publish_reply(msg: &BrokerMessage) -> Result<(), String> {
    otel_wasi::attribute!(
        "messaging.system" = "nats",
        "messaging.message.body.size" = msg.body.len() as i64,
    );

    Ok(())
}
```

Because the main span is entered by `#[wasi_instrument]`, child `#[tracing::instrument]` spans become children automatically.

## Attributes

Use `attribute!` to set one or more attributes on the current span:

```rust
otel_wasi::attribute!(
    "messaging.system" = "nats",
    "messaging.message.body.size" = body.len() as i64,
);
```

For manual flows, the macro also supports an explicit span target:

```rust
otel_wasi::attribute!(
    span: my_span.span(),
    "messaging.system" = "nats",
);
```

Static root-span attributes can be placed directly on `#[wasi_instrument(...)]` using `attributes(...)`. Dynamic values should be set inside the function body with `attribute!`.

## Outcome behavior

The planned default behavior is:

- `()` return: mark span OK.
- Plain value return: mark span OK.
- `Result<T, E>` return:
  - `Ok(_)`: mark span OK.
  - `Err(e)`: mark span error, record `error = true`, `exception.slug`, and `exception.message`.

The macro internally wraps the function body so `?` returns are still recorded before returning to the caller. For native `async fn`, the generated code instruments the inner future with `tracing::Instrument` rather than holding a span-enter guard across `.await` points.

## Manual API

The macro is the preferred API, but the runtime should remain usable manually:

```rust
let span = otel_wasi::WasiSpan::start(
    otel_wasi::SpanConfig::builder()
        .service_name("nats-echo")
        .span_name("handle-message")
        .error_slug("nats-publish-failed")
        .build(),
);

let result = {
    let _guard = span.enter();
    publish_reply(&msg)
};

span.finish(&result);
result
```

This keeps the proc macro replaceable and the runtime maintainable long-term.

## Examples

See:

```text
examples/wasmcloud-nats-echo
```

for a wasmCloud messaging component example. It can be checked with Cargo and built with `wash build`.

## Development status

This crate is under active development. See:

```text
docs/wasi-instrument-developer-experience-plan.md
```

for the implementation plan.
