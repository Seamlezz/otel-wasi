# wasmCloud HTTP hello example

A minimal `wasi:http/handler@0.3.0` example showing how to use `#[otel_wasi::wasi_instrument]` with a typed WIT error (`ErrorCode`) instead of the default `String` error.

It shows:

- `#[otel_wasi::wasi_instrument]` with `export` on an `async fn` handler.
- Static root span attributes via `attributes(...)`.
- Dynamic root span attributes via `otel_wasi::main_attribute!(...)`.
- Error slugging with `error_with_slug` on a `Result<T, ErrorCode>`.
- How `export` rewrites the WIT signature from `Result<Response, otel_wasi::Error<ErrorCode>>` to `Result<Response, ErrorCode>`.

## Prerequisites

- [Rust](https://rustup.rs/).
- The `wasm32-wasip2` target: `rustup target add wasm32-wasip2`.
- [wasmCloud wash](https://wasmcloud.com/docs/installation) CLI 2.x.

## Build

From this directory:

```sh
wash build
```

From the repository root:

```sh
wash -C examples/wasmcloud-http-hello build
```

The `.wash/config.yaml` in this directory tells wash to run `cargo build --target wasm32-wasip2 --release`.

> `wash build` needs the `wasi:http@0.3.0` WIT dependencies. If wash fails to fetch them, place the matching local WIT packages in `wit/deps` or use a wash version compatible with `wasi:http@0.3.0`.

## Key point

The handler body returns:

```rust
Result<Response, otel_wasi::Error<ErrorCode>>
```

With `#[wasi_instrument(export)]`, the exported WIT signature becomes:

```rust
Result<Response, ErrorCode>
```

The slug and message are recorded on the main span, and the original `ErrorCode` is returned to the WASI HTTP runtime via `e.into_inner()`.

## Running the component

Deploy the built component to a wasmCloud host and route traffic to it. This README covers building only; see the wasmCloud documentation for deployment steps.

See the main [README](../../README.md) for the full `otel-wasi` API, concepts, and the dylint lint.
