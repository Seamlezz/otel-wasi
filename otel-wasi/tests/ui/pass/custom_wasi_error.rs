use std::fmt;
use otel_wasi::{wasi_instrument, WasiError};

#[derive(Debug)]
struct MyError {
    slug: &'static str,
    message: String,
}

impl fmt::Display for MyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl WasiError for MyError {
    fn slug(&self) -> &'static str {
        self.slug
    }

    fn message(&self) -> &str {
        &self.message
    }
}

#[wasi_instrument(service = "test-service")]
fn uses_custom_error() -> Result<(), MyError> {
    Err(MyError {
        slug: "custom-failed",
        message: "custom error message".to_string(),
    })
}

fn main() {}
