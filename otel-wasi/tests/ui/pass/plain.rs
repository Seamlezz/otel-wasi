use otel_wasi::wasi_instrument;

#[wasi_instrument(service = "test-service")]
fn plain() -> u64 {
    42
}

fn main() {}
