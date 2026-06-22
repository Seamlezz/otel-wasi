use otel_wasi::wasi_instrument;

#[wasi_instrument(service = "test-service")]
fn unit() {}

fn main() {}
