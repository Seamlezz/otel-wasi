use otel_wasi::{WithSlug, wasi_error};

fn main() {
    let e = wasi_error!("db-timeout", "connection lost");
    let _e2 = e.with_slug("http-timeout");
}
