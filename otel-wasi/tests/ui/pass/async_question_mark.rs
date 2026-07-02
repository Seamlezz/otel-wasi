use otel_wasi::wasi_instrument;

async fn may_fail_async() -> Result<(), otel_wasi::Error> {
    Ok(())
}

#[wasi_instrument(service = "test-service")]
async fn question_mark_async() -> Result<(), otel_wasi::Error> {
    may_fail_async().await?;
    Ok(())
}

fn main() {}
