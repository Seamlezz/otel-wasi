use otel_wasi::wasi_instrument;

#[wasi_instrument(service = "test-service")]
fn records_main_attribute() -> Result<(), otel_wasi::Error> {
    child_span();
    Ok(())
}

#[tracing::instrument(name = "child-span")]
fn child_span() {
    otel_wasi::main_attribute!("test.value" = 1_i64);
}

fn main() {}
