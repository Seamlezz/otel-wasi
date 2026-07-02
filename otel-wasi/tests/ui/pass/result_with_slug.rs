use otel_wasi::{wasi_instrument, ResultWithSlug};

fn external_call() -> Result<(), String> {
    Err("external error".to_string())
}

#[wasi_instrument(service = "test-service")]
fn uses_result_with_slug() -> Result<(), otel_wasi::Error> {
    external_call().error_with_slug("external-call-failed")?;
    Ok(())
}

fn main() {}
