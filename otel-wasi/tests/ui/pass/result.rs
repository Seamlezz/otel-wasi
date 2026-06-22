use otel_wasi::wasi_instrument;

#[wasi_instrument(
    service = "test-service",
    name = "fallible",
    error_slug = "fallible-failed",
    attributes("component.kind" = "test")
)]
fn fallible() -> Result<(), String> {
    Ok(())
}

fn main() {}
