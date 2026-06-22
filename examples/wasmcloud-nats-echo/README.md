# wasmCloud NATS echo example

This example shows how a wasmCloud messaging component can use `otel-wasi`.

It demonstrates:

- `#[otel_wasi::wasi_instrument]` on the wasmCloud exported handler.
- Static root-span attributes via `attributes(...)`.
- Dynamic current-span attributes via `otel_wasi::attribute!(...)`.
- Normal `#[tracing::instrument]` on child functions.
- Error recording on child spans.

## Creating a similar component with wash

Depending on your `wash` version, project creation is done with either `wash new` or older docs/tools may call this workflow `wash init`.

With `wash 2.x`, use `wash new` with a wasmCloud component template, then add `otel-wasi`:

```bash
wash new <template-git-url> --name my-component
cd my-component
cargo add otel-wasi --path /path/to/otel-wasi/otel-wasi
```

Then instrument your exported handler:

```rust
impl bindings::exports::wasmcloud::messaging::handler::Guest for Component {
    #[otel_wasi::wasi_instrument(
        service = "my-component",
        name = "handle-message",
        error_slug = "message-handler-failed",
        attributes("messaging.system" = "nats")
    )]
    fn handle_message(msg: BrokerMessage) -> Result<(), String> {
        otel_wasi::attribute!(
            "messaging.destination.name" = msg.subject.clone(),
            "messaging.message.body.size" = msg.body.len() as i64,
        );

        Ok(())
    }
}
```

## Building this example

From this directory:

```bash
cargo check
wash build
```

This example includes `.wash/config.yaml`, so `wash build` knows to run:

```bash
cargo build --target wasm32-wasip2 --release
```

Or from the repository root:

```bash
cargo check --manifest-path examples/wasmcloud-nats-echo/Cargo.toml
wash -C examples/wasmcloud-nats-echo build
```

## Notes

`#[wasi_instrument]` is meant for the main/root WASI entry span. For child functions, keep using normal `tracing` instrumentation:

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
