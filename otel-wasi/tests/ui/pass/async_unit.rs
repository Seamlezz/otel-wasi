use otel_wasi::wasi_instrument;

#[wasi_instrument(service = "test-service")]
async fn unit_async() {}

fn main() {}
