use otel_wasi::wasi_instrument;

fn may_fail() -> Result<(), otel_wasi::Error> {
    Ok(())
}

#[wasi_instrument(service = "test-service")]
fn question_mark() -> Result<(), otel_wasi::Error> {
    may_fail()?;
    Ok(())
}

fn main() {}
