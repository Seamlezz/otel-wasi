use otel_wasi::{PropagationContext, wasi_instrument};

#[wasi_instrument(service = "test-service", parent = parent)]
fn linked_entrypoint(parent: Option<PropagationContext>) {
    drop(parent);
}

fn main() {}
