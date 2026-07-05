use otel_wasi::{wasi_instrument, WithSlug};

#[derive(Debug, PartialEq)]
struct MyCode(u32);

impl std::fmt::Display for MyCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "code {}", self.0)
    }
}

#[wasi_instrument(service = "test-service", export)]
fn fallible_export(code: u32) -> Result<(), otel_wasi::Error<MyCode>> {
    Err(MyCode(code).with_slug("my-slug"))
}

fn main() {
    // Verify that `#[wasi_instrument(export)]` rewrote the signature from
    // `Result<(), otel_wasi::Error<MyCode>>` to `Result<(), MyCode>`.
    let _: fn(u32) -> Result<(), MyCode> = fallible_export;
}
