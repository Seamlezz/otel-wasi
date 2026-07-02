use otel_wasi::wasi_instrument;

#[wasi_instrument(
    service = "test-service",
    name = "fallible",
    attributes("component.kind" = "test")
)]
fn fallible() -> Result<(), otel_wasi::Error> {
    Ok(())
}

fn main() {}
