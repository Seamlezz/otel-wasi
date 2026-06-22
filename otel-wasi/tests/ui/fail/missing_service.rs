use otel_wasi::wasi_instrument;

#[wasi_instrument(name = "missing-service")]
fn missing_service() {}

fn main() {}
