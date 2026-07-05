# wasmCloud NATS echo example

A wasmCloud messaging component that uses `#[otel_wasi::wasi_instrument]` on its exported handler.

It shows:

- `#[otel_wasi::wasi_instrument]` with `export` on a synchronous handler.
- Static root span attributes via `attributes(...)`.
- Dynamic current span attributes via `otel_wasi::attribute!(...)`.
- Normal `#[tracing::instrument]` on child functions.
- Error slugging with `error_with_slug` and conversion back to the WIT string error.

## Prerequisites

- [Rust](https://rustup.rs/).
- The `wasm32-wasip2` target: `rustup target add wasm32-wasip2`.
- [wasmCloud wash](https://wasmcloud.com/docs/installation) CLI 2.x.

## Build

From this directory:

```sh
cargo check
wash build
```

`cargo check` validates the Rust code. `wash build` produces the WebAssembly component using the `.wash/config.yaml` in this directory.

From the repository root:

```sh
cargo check --manifest-path examples/wasmcloud-nats-echo/Cargo.toml
wash -C examples/wasmcloud-nats-echo build
```

The built component is written to `../../target/wasm32-wasip2/release/otel_wasi_wasmcloud_nats_echo_example.wasm`.

## Create a similar component with wash

With wash 2.x, create a new component from a wasmCloud template, then add `otel-wasi`:

```sh
wash new <template-git-url> --name my-component
cd my-component
cargo add otel-wasi --git https://github.com/Seamlezz/otel-wasi
```

Instrument the exported handler:

```rust
impl bindings::exports::wasmcloud::messaging::handler::Guest for Component {
    #[otel_wasi::wasi_instrument(
        service = "my-component",
        name = "handle-message",
        export,
        attributes("messaging.system" = "nats")
    )]
    fn handle_message(msg: BrokerMessage) -> Result<(), otel_wasi::Error> {
        otel_wasi::attribute!(
            "messaging.destination.name" = msg.subject.clone(),
            "messaging.message.body.size" = msg.body.len() as i64,
        );

        Ok(())
    }
}
```

## Notes

`#[wasi_instrument]` is meant for the main/root WASI entry span. For child functions, keep using normal `tracing` instrumentation:

```rust
#[tracing::instrument(name = "publish-reply", level = "info", skip(msg))]
fn publish_reply(msg: &BrokerMessage) -> Result<(), otel_wasi::Error> {
    otel_wasi::attribute!(
        "messaging.system" = "nats",
        "messaging.message.body.size" = msg.body.len() as i64,
    );

    Ok(())
}
```

See the main [README](../../README.md) for the full `otel-wasi` API, concepts, and the dylint lint.
