use otel_wasi::wasi_instrument;

#[wasi_instrument(service = "test-service")]
async fn fallible_async() -> Result<(), otel_wasi::Error> {
    Ok(())
}

fn main() {}
