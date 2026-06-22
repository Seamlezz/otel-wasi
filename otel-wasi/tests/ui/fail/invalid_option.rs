use otel_wasi::wasi_instrument;

#[wasi_instrument(service = "test-service", nope = "nope")]
fn invalid_option() {}

fn main() {}
