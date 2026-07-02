use otel_wasi::{wasi_instrument, WithSlug};

fn external_call() -> Result<(), String> {
    Err("external error".to_string())
}

#[wasi_instrument(service = "test-service")]
fn uses_with_slug() -> Result<(), otel_wasi::Error> {
    external_call().map_err(|e| e.with_slug("external-call-failed"))?;
    Ok(())
}

fn main() {}
