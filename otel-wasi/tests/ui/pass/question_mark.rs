use otel_wasi::wasi_instrument;

fn may_fail() -> Result<(), String> {
    Ok(())
}

#[wasi_instrument(service = "test-service")]
fn question_mark() -> Result<(), String> {
    may_fail()?;
    Ok(())
}

fn main() {}
