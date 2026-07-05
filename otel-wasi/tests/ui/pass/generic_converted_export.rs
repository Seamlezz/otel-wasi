use otel_wasi::{ResultWithSlug, WithSlug, wasi_instrument};

#[derive(Debug, PartialEq)]
struct MyCode(u32);

impl std::fmt::Display for MyCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "code {}", self.0)
    }
}

#[wasi_instrument(service = "test-service", export)]
fn fallible_export(code: u32) -> Result<(), otel_wasi::Error> {
    Err(MyCode(code).with_slug("my-slug"))
}

#[wasi_instrument(service = "test-service", export)]
fn fallible_export_2(code: u32) -> Result<(), otel_wasi::Error> {
    Err(MyCode(code)).error_with_slug("my-slug")
}

fn main() {
    let _: fn(u32) -> Result<(), String> = fallible_export;
    let _: fn(u32) -> Result<(), String> = fallible_export_2;
}
