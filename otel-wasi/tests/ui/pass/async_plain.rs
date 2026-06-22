use otel_wasi::wasi_instrument;

#[wasi_instrument(service = "test-service")]
async fn plain_async() -> u64 {
    42
}

fn main() {}
