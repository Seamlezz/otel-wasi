use otel_wasi::{ResultWithSlug, wasi_error};

fn external_call() -> Result<(), otel_wasi::Error> {
    Err(wasi_error!("db-timeout", "connection lost"))
}

fn main() {
    let _ = external_call().error_with_slug("http-timeout");
}
