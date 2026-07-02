use otel_wasi::{wasi_error, wasi_instrument};

#[wasi_instrument(service = "test-service")]
fn uses_wasi_error_macro() -> Result<(), otel_wasi::Error> {
    Err(wasi_error!("my-slug", "something went wrong"))
}

fn main() {}
